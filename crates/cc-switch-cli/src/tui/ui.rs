use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};
use ratatui::Frame;
use unicode_width::UnicodeWidthChar;

use super::{provider::FIELDS, settings::LABELS, App};

fn clean(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

fn list(frame: &mut Frame, area: Rect, title: &str, rows: Vec<String>, selected: usize) {
    let mut state = ListState::default().with_selected(Some(selected));
    let widget = List::new(rows.into_iter().map(|s| ListItem::new(clean(&s))))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(title.to_owned()),
        )
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("› ");
    frame.render_stateful_widget(widget, area, &mut state);
}

pub(super) fn draw(frame: &mut Frame, app: &App) {
    let area = frame.area();
    if area.width < 48 || area.height < 12 {
        frame.render_widget(
            Paragraph::new(if app.confirm.is_some() {
                "丢弃修改？Y 确认 / N 返回"
            } else {
                "请扩大终端至至少 48 列 × 12 行；Ctrl+C 退出"
            })
            .wrap(Wrap { trim: false }),
            area,
        );
        return;
    }
    let regions = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(4),
        Constraint::Length(3),
        Constraint::Length(2),
    ])
    .split(area);
    let title = format!(
        "CC Switch · {}    {}{}",
        app.app().as_str(),
        if app.settings_page {
            "供应商 | [CLI 设置]"
        } else {
            "[供应商] | CLI 设置"
        },
        if app.dirty() { "  * 未保存" } else { "" }
    );
    frame.render_widget(
        Paragraph::new(title).style(Style::default().fg(Color::Cyan)),
        regions[0],
    );

    let footer = if let Some(editor) = &app.editor {
        let draft = app.draft.as_ref().unwrap();
        let block = Block::default()
            .borders(Borders::ALL)
            .title(FIELDS[draft.field]);
        let inner = block.inner(regions[1]);
        frame.render_widget(block, regions[1]);
        let height = usize::from(inner.height).max(1);
        let width = usize::from(inner.width).max(1);
        let top = editor.row.saturating_sub(height - 1);
        let masked = editor.secret && !editor.reveal;
        let cursor_x: usize = editor.lines[editor.row]
            .chars()
            .take(editor.column)
            .map(|c| if masked { 1 } else { c.width().unwrap_or(0) })
            .sum();
        let left = cursor_x.saturating_sub(width - 1);
        let rows: Vec<String> = editor
            .lines
            .iter()
            .skip(top)
            .take(height)
            .map(|line| {
                let mut column = 0;
                let mut result = String::new();
                for ch in line.chars() {
                    let ch = if masked {
                        '*'
                    } else if ch.is_control() {
                        ' '
                    } else {
                        ch
                    };
                    let span = ch.width().unwrap_or(0);
                    if column >= left && column + span <= left + width {
                        result.push(ch);
                    } else if column < left && column + span > left {
                        result.push_str(&" ".repeat(column + span - left));
                    }
                    column += span;
                }
                result
            })
            .collect();
        frame.render_widget(Paragraph::new(rows.join("\n")), inner);
        frame.set_cursor_position((
            inner.x + (cursor_x - left) as u16,
            inner.y + (editor.row - top) as u16,
        ));
        if editor.multiline {
            "Ctrl+S 应用到草稿  Esc 取消  Ctrl+U 清空  F1 帮助"
        } else {
            "Enter 应用到草稿  Esc 取消  Ctrl+U 清空  F2 显示/隐藏 Key"
        }
    } else if let Some(draft) = &app.draft {
        let rows = (0..draft.field_count())
            .map(|i| {
                let value = if i == 2 {
                    if draft.field_text(i).is_empty() {
                        "（空）".into()
                    } else {
                        "********".into()
                    }
                } else if i >= 4 {
                    "Enter 编辑".into()
                } else {
                    draft.field_text(i)
                };
                format!("{}: {}", FIELDS[i], value)
            })
            .collect();
        list(
            frame,
            regions[1],
            &format!("编辑供应商 · {}", clean(&draft.original.id)),
            rows,
            draft.field,
        );
        "↑↓/Tab 选择字段  Enter 编辑  Ctrl+S 保存  Esc 返回  ? 帮助"
    } else if app.settings_page {
        if let Some(settings) = &app.settings {
            let rows = LABELS
                .iter()
                .enumerate()
                .map(|(i, label)| {
                    format!("{label}: {}", if settings.values[i] { "开" } else { "关" })
                })
                .collect();
            list(
                frame,
                regions[1],
                "CLI 设置 · 重启后台代理后生效",
                rows,
                settings.selected,
            );
        } else {
            frame.render_widget(
                Paragraph::new("设置读取失败。修正 cli.json 后按 r 刷新。")
                    .block(Block::bordered().title("CLI 设置")),
                regions[1],
            );
        }
        "↑↓ 选择  Space/Enter 切换  Ctrl+S 保存  Tab 供应商  r 刷新  ? 帮助"
    } else {
        let rows = app
            .providers
            .iter()
            .map(|p| {
                format!(
                    "{} {}  ({})",
                    if p.id == app.current { "*" } else { " " },
                    p.name,
                    p.id
                )
            })
            .collect();
        if app.providers.is_empty() {
            frame.render_widget(
                Paragraph::new("暂无供应商。请先在桌面端添加或导入，然后按 r 刷新。")
                    .wrap(Wrap { trim: false })
                    .block(Block::bordered().title("供应商")),
                regions[1],
            );
        } else {
            list(frame, regions[1], "供应商", rows, app.selected);
        }
        "↑↓ 选择  Enter/e 编辑  [ ] 切换应用  Tab CLI 设置  r 刷新  q 退出"
    };
    frame.render_widget(
        Paragraph::new(clean(&app.status))
            .wrap(Wrap { trim: false })
            .style(Style::default().fg(Color::Yellow)),
        regions[2],
    );
    frame.render_widget(
        Paragraph::new(footer).wrap(Wrap { trim: false }),
        regions[3],
    );
    if app.confirm.is_some() || app.help {
        let text = if app.confirm.is_some() {
            "丢弃未保存的修改？\n\nY 丢弃并继续    N / Esc 返回编辑"
        } else {
            "供应商：Enter/e 打开，↑↓ 选择字段，Enter 编辑。\n单行 Enter、多行 Ctrl+S 将字段写入草稿；再按 Ctrl+S 保存整个供应商。\nCtrl+U 清空字段；支持方向键、Home/End、PageUp/PageDown 和终端粘贴。Key 默认隐藏，F2 切换显示；原始 JSON 含凭据。\n\nCodex TOML 包含原生全局设置；保存全局修改会影响所有供应商。外部冲突会拒绝保存，草稿保留。Antigravity 下次 launch 使用新配置。\n\nTab 切换供应商/CLI 设置；[ ] 按 CLI 白名单切换应用。CLI 开关仅影响 CLI 后台代理，保存后需 proxy disable，再 proxy enable。\n\nEsc 返回；q 或 Ctrl+C 退出。有修改时会确认丢弃。任意键关闭帮助。"
        };
        let popup = Rect::new(
            area.x + 2,
            area.y + 2,
            area.width.saturating_sub(4),
            area.height.saturating_sub(4),
        );
        frame.render_widget(Clear, popup);
        frame.render_widget(
            Paragraph::new(text)
                .wrap(Wrap { trim: false })
                .block(Block::bordered().title(if app.help {
                    "帮助"
                } else {
                    "未保存的修改"
                })),
            popup,
        );
    }
}
