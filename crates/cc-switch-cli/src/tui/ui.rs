use super::{data::Page, keymap, provider::FIELDS, settings::LABELS, theme::Theme, App, Confirm};
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, BorderType, Cell, Clear, Padding, Paragraph, Row, Table, TableState, Wrap,
};
use ratatui::Frame;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

mod content;
mod editing;

fn clean(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

fn fit(text: &str, width: usize) -> String {
    let text = clean(text);
    if text.width() <= width {
        return text;
    }
    if width == 0 {
        return String::new();
    }
    let mut used = 0;
    let mut out = String::new();
    for c in text.chars() {
        let w = c.width().unwrap_or(0);
        if used + w > width - 1 {
            break;
        }
        out.push(c);
        used += w;
    }
    out.push('…');
    out
}

fn panel(title: impl Into<String>, theme: &Theme, focus: bool) -> Block<'static> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if focus { theme.accent } else { theme.border }))
        .title(Line::styled(
            format!(" {} ", title.into()),
            Style::default()
                .fg(if focus { theme.accent } else { theme.muted })
                .add_modifier(Modifier::BOLD),
        ))
        .padding(Padding::horizontal(1))
}

fn keys(frame: &mut Frame, area: Rect, items: &[(&str, &str)], theme: &Theme) {
    let mut spans = vec![];
    let mut used = 0usize;
    for (i, (key, label)) in items.iter().enumerate() {
        let width = key.width() + label.width() + 4;
        let reserve = if i + 1 < items.len() { 9 } else { 0 };
        if used + width + reserve > area.width as usize {
            spans.push(Span::styled(" ? 更多", Style::default().fg(theme.muted)));
            break;
        }
        spans.push(Span::styled(
            format!(" {key} "),
            Style::default()
                .fg(theme.accent)
                .bg(theme.surface)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(Span::styled(
            format!(" {label} "),
            Style::default().fg(theme.muted),
        ));
        used += width;
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn table(
    frame: &mut Frame,
    area: Rect,
    headers: &[&str],
    rows: Vec<Vec<String>>,
    selected: usize,
    widths: Vec<Constraint>,
    theme: &Theme,
    focus: bool,
) {
    let rows = rows.into_iter().map(|row| {
        Row::new(row.into_iter().map(|v| Cell::from(clean(&v))))
            .height(1)
            .bottom_margin(1)
    });
    let widget = Table::new(rows, widths)
        .header(
            Row::new(headers.iter().map(|s| Cell::from(*s)))
                .style(Style::default().fg(theme.muted))
                .bottom_margin(1),
        )
        .column_spacing(2)
        .row_highlight_style(theme.selected(focus))
        .highlight_symbol("› ");
    let mut state =
        TableState::default().with_selected((selected != usize::MAX).then_some(selected));
    frame.render_stateful_widget(widget, area, &mut state);
}

fn empty(frame: &mut Frame, area: Rect, title: &str, subtitle: &str, theme: &Theme) {
    let top = area.height.saturating_sub(4) / 2;
    let area = Rect {
        y: area.y + top,
        height: area.height.saturating_sub(top),
        ..area
    };
    frame.render_widget(
        Paragraph::new(vec![
            Line::styled(
                title,
                Style::default().fg(theme.fg).add_modifier(Modifier::BOLD),
            ),
            Line::raw(""),
            Line::styled(subtitle, Style::default().fg(theme.muted)),
        ])
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: false }),
        area,
    );
}

pub(super) fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    let theme = Theme::for_app(&app.app());
    frame.render_widget(Block::default().style(theme.base()), area);
    if area.width < 48 || area.height < 12 {
        let text = if app.confirm.is_some() {
            "确认操作？Y 确认 / N 返回"
        } else {
            "请扩大终端至至少 48 列 × 12 行\nCtrl+C 退出"
        };
        frame.render_widget(Paragraph::new(text).wrap(Wrap { trim: false }), area);
        return;
    }
    let root = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(5),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(area);
    header(frame, root[0], app, &theme);
    let content = if area.width >= 88 {
        let columns =
            Layout::horizontal([Constraint::Length(20), Constraint::Min(30)]).split(root[1]);
        navigation(frame, columns[0], app, &theme);
        columns[1]
    } else {
        root[1]
    };
    if area.width < 88 && app.nav_focus && app.draft.is_none() && app.form.is_none() {
        navigation(frame, content, app, &theme);
    } else {
        page(frame, content, app, &theme);
    }
    let status = if let Some(busy) = &app.busy {
        format!(" ◌ {busy}…")
    } else if !app.status.is_empty() {
        format!(
            " {}  {}",
            if app.error { "!" } else { "✓" },
            app.status.lines().next().unwrap_or_default()
        )
    } else if app.dirty() {
        " ● 未保存的修改".into()
    } else {
        format!(" {}  /  {}", app.app().as_str(), app.page.title())
    };
    let status_area =
        Layout::horizontal([Constraint::Min(1), Constraint::Length(12)]).split(root[2]);
    frame.render_widget(
        Paragraph::new(fit(&status, status_area[0].width as usize)).style(Style::default().fg(
            if app.error {
                theme.error
            } else if app.busy.is_some() {
                theme.warn
            } else {
                theme.muted
            },
        )),
        status_area[0],
    );
    if !app.status.is_empty() {
        frame.render_widget(
            Paragraph::new("F4 操作详情")
                .alignment(Alignment::Right)
                .style(Style::default().fg(theme.muted)),
            status_area[1],
        );
    }
    keys(
        frame,
        root[3],
        &[
            ("Tab/←→", "菜单/内容"),
            ("[ ]", "应用"),
            ("1–9", "页面"),
            ("?", "帮助"),
            ("q", "退出"),
        ],
        &theme,
    );
    if let Some(confirm) = &app.confirm {
        let text = match confirm {
            Confirm::Discard(_) => "丢弃未保存的修改？".into(),
            Confirm::Action(text, _) => clean(text),
        };
        overlay(
            frame,
            area,
            "确认操作",
            &format!("{text}\n\nY 确认    N / Esc 返回"),
            0,
            10,
            &theme,
        );
    } else if app.help {
        overlay(
            frame,
            area,
            "帮助 · ↑↓ / PgUp / PgDn 滚动 · Esc 关闭",
            &editing::help(app),
            app.help_scroll,
            area.height.saturating_sub(2),
            &theme,
        );
    } else if let Some(viewer) = &app.viewer {
        overlay(
            frame,
            area,
            &format!("{} · ↑↓ 滚动 · Esc 关闭", clean(&viewer.title)),
            &viewer.text,
            viewer.scroll,
            area.height.saturating_sub(2),
            &theme,
        );
    }
}

fn header(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let block = panel("", theme, false);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let title_width = if area.width >= 70 { 22 } else { 13 };
    let chunks = Layout::horizontal([
        Constraint::Length(title_width),
        Constraint::Min(20),
        Constraint::Length(if area.width >= 90 { 29 } else { 0 }),
    ])
    .split(inner);
    let title = if area.width >= 70 {
        "CC-SWITCH  /  TERMINAL"
    } else {
        "CC-SWITCH"
    };
    frame.render_widget(
        Paragraph::new(title).style(Style::default().fg(theme.fg).add_modifier(Modifier::BOLD)),
        chunks[0],
    );
    let mut spans = vec![];
    for (index, app_type) in app.apps.iter().enumerate() {
        let label = match app_type {
            cc_switch_core::AppType::Codex => "Codex",
            cc_switch_core::AppType::Agy => "Antigravity",
            _ => app_type.as_str(),
        };
        spans.push(Span::styled(
            format!(" {label} "),
            if index == app.app_index {
                theme.selected(true)
            } else {
                Style::default().fg(theme.muted).bg(theme.surface)
            },
        ));
        spans.push(Span::raw("  "));
    }
    frame.render_widget(
        Paragraph::new(Line::from(spans)).alignment(Alignment::Center),
        chunks[1],
    );
    if chunks[2].width > 0 {
        let name = app
            .providers
            .iter()
            .find(|p| p.id == app.current)
            .map(|p| p.name.as_str())
            .unwrap_or("未选择");
        let label = if app.app() == cc_switch_core::AppType::Agy {
            "独立启动配置".into()
        } else {
            format!("当前 · {}", fit(name, 18))
        };
        frame.render_widget(
            Paragraph::new(label)
                .alignment(Alignment::Right)
                .style(Style::default().fg(theme.accent)),
            chunks[2],
        );
    }
}

fn navigation(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let block = panel("菜单", theme, app.nav_focus);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let rows = Page::NAV.iter().enumerate().map(|(i, page)| {
        Row::new(vec![format!("{}", i + 1), page.title().to_owned()])
            .height(if inner.height >= 20 { 2 } else { 1 })
    });
    let widget = Table::new(rows, [Constraint::Length(2), Constraint::Min(10)])
        .column_spacing(1)
        .row_highlight_style(theme.selected(app.nav_focus))
        .highlight_symbol("› ");
    let mut state = TableState::default().with_selected(Some(if app.nav_focus {
        app.nav_index
    } else {
        app.page.nav_index()
    }));
    frame.render_stateful_widget(widget, inner, &mut state);
}

fn page(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let title = if let Some(draft) = &app.draft {
        format!(
            "供应商 › {}{}",
            if draft.is_new { "新增" } else { "编辑" },
            if draft.dirty() { "  ●" } else { "" }
        )
    } else if let Some(form) = &app.form {
        format!(
            "{} › {}{}",
            app.page.title(),
            form.title,
            if form.dirty() { "  ●" } else { "" }
        )
    } else {
        app.page.title().into()
    };
    let block = panel(title, theme, !app.nav_focus);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let regions = Layout::vertical([Constraint::Length(2), Constraint::Min(1)]).split(inner);
    let bindings = keymap::bindings(app.page, &app.app());
    let key_items: Vec<_> = if let Some(editor) = &app.editor {
        vec![
            (
                if editor.multiline { "Ctrl+S" } else { "Enter" },
                "应用到草稿",
            ),
            ("Esc", "取消"),
            ("Ctrl+U", "清空"),
            (
                if editor.secret { "F2" } else { "F1" },
                if editor.secret {
                    "显示/隐藏"
                } else {
                    "帮助"
                },
            ),
        ]
    } else if app.draft.is_some() || app.form.is_some() {
        vec![
            ("↑↓/Tab", "字段"),
            ("Enter", "编辑"),
            ("Ctrl+S", "保存"),
            ("Esc", "返回"),
        ]
    } else if app.page == Page::Settings {
        vec![
            ("↑↓", "选择"),
            ("Space", "切换"),
            ("Ctrl+S", "保存"),
            ("r", "刷新"),
        ]
    } else {
        bindings.iter().map(|b| (b.label, b.hint)).collect()
    };
    keys(
        frame,
        Rect {
            height: 1,
            ..regions[0]
        },
        &key_items,
        theme,
    );
    let mut body = regions[1];
    if app.editor.is_some() {
        editing::editor(frame, body, app, theme);
        return;
    }
    if app.draft.is_some() || app.form.is_some() {
        editing::form(frame, body, app, theme);
        return;
    }
    if app.filtering || !app.filter.is_empty() {
        let chunks = Layout::vertical([Constraint::Length(2), Constraint::Min(1)]).split(body);
        frame.render_widget(
            Paragraph::new(format!(
                " / {}{}",
                clean(&app.filter),
                if app.filtering { "▏" } else { "" }
            ))
            .style(Style::default().fg(theme.accent).bg(theme.surface)),
            Rect {
                height: 1,
                ..chunks[0]
            },
        );
        body = chunks[1];
    }
    content::render(frame, body, app, theme);
}

fn overlay(
    frame: &mut Frame,
    area: Rect,
    title: &str,
    text: &str,
    scroll: u16,
    height: u16,
    theme: &Theme,
) {
    let width = area.width.saturating_sub(4).min(100);
    let height = height.min(area.height.saturating_sub(2));
    let popup = Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    );
    frame.render_widget(Clear, popup);
    let block = panel(title, theme, true).style(theme.base());
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    let mut lines = Vec::new();
    for line in text.lines() {
        let mut row = String::new();
        let mut width = 0;
        for ch in clean(line).chars() {
            let size = ch.width().unwrap_or(0);
            if width + size > inner.width as usize && !row.is_empty() {
                lines.push(Line::raw(std::mem::take(&mut row)));
                width = 0;
            }
            row.push(ch);
            width += size;
        }
        lines.push(Line::raw(row));
    }
    let max_scroll = lines
        .len()
        .saturating_sub(inner.height as usize)
        .min(u16::MAX as usize) as u16;
    frame.render_widget(
        Paragraph::new(lines).scroll((scroll.min(max_scroll), 0)),
        inner,
    );
}
