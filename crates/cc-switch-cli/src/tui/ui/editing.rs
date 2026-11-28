use super::*;

pub(super) fn form(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let (rows, selected) = if let Some(draft) = &app.draft {
        let rows = (0..draft.field_count())
            .map(|i| {
                let value = match i {
                    2 => {
                        if draft.field_text(i).is_empty() {
                            "未设置".into()
                        } else {
                            "********".into()
                        }
                    }
                    4.. => "Enter 打开编辑器".into(),
                    _ => {
                        let text = draft.field_text(i);
                        if text.is_empty() {
                            "未设置".into()
                        } else {
                            text
                        }
                    }
                };
                vec![FIELDS[i].to_owned(), value]
            })
            .collect();
        (rows, draft.field)
    } else {
        let form = app.form.as_ref().unwrap();
        let rows = form
            .fields
            .iter()
            .map(|f| {
                vec![
                    format!("{}{}", f.label, if f.readonly { "（只读）" } else { "" }),
                    if f.toggle {
                        if f.value == "true" {
                            "● 开启".into()
                        } else {
                            "○ 关闭".into()
                        }
                    } else if f.multiline {
                        format!("{} 行 · Enter 编辑", f.value.lines().count())
                    } else if f.value.is_empty() {
                        "未设置".into()
                    } else {
                        f.value.clone()
                    },
                ]
            })
            .collect();
        (rows, form.selected)
    };
    table(
        frame,
        area,
        &["字段", "当前值"],
        rows,
        selected,
        vec![Constraint::Percentage(43), Constraint::Min(12)],
        theme,
        true,
    );
}

pub(super) fn editor(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let editor = app.editor.as_ref().unwrap();
    let title = if let Some(draft) = &app.draft {
        FIELDS[draft.field].to_owned()
    } else {
        app.form.as_ref().unwrap().fields[app.form.as_ref().unwrap().selected]
            .label
            .clone()
    };
    let block = panel(title, theme, true).title_bottom(
        Line::from(format!(
            " 行 {}  列 {}  {} ",
            editor.row + 1,
            editor.column + 1,
            if editor.secret && !editor.reveal {
                "凭据已隐藏"
            } else {
                ""
            }
        ))
        .right_aligned(),
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.height == 0 || inner.width == 0 {
        return;
    }
    let gutter = if editor.multiline && inner.width > 20 {
        5
    } else {
        0
    };
    let text_area = Rect {
        x: inner.x + gutter,
        width: inner.width - gutter,
        ..inner
    };
    let height = usize::from(text_area.height);
    let width = usize::from(text_area.width);
    let top = editor.row.saturating_sub(height - 1);
    let masked = editor.secret && !editor.reveal;
    let cursor_x: usize = editor.lines[editor.row]
        .chars()
        .take(editor.column)
        .map(|c| {
            if masked || c.is_control() {
                1
            } else {
                c.width().unwrap_or(0)
            }
        })
        .sum();
    let left = cursor_x.saturating_sub(width - 1);
    let rows: Vec<Line> = editor
        .lines
        .iter()
        .enumerate()
        .skip(top)
        .take(height)
        .map(|(row, line)| {
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
            Line::styled(
                result,
                if row == editor.row {
                    Style::default().bg(theme.surface)
                } else {
                    Style::default()
                },
            )
        })
        .collect();
    frame.render_widget(Paragraph::new(rows), text_area);
    if gutter > 0 {
        let numbers: Vec<Line> = (top..(top + height).min(editor.lines.len()))
            .map(|n| Line::styled(format!("{:>3} │", n + 1), Style::default().fg(theme.muted)))
            .collect();
        frame.render_widget(
            Paragraph::new(numbers),
            Rect {
                width: gutter,
                ..inner
            },
        );
    }
    if app.confirm.is_none() && !app.help && app.viewer.is_none() {
        frame.set_cursor_position((
            text_area.x + (cursor_x - left) as u16,
            text_area.y + (editor.row - top) as u16,
        ));
    }
}

pub(super) fn help(app: &App) -> String {
    let mut text = format!("{} · 可用操作\n\n", app.page.title());
    for binding in keymap::bindings(app.page, &app.app()) {
        text.push_str(&format!("  {:<10} {}\n", binding.label, binding.hint));
    }
    text.push_str("\n导航\n  Tab / ← →  菜单与内容之间切换焦点\n  ↑ ↓ / j k  选择项目；Home / End 跳到首尾\n  1–9        直接打开对应页面\n  [ ]        切换应用\n  /          搜索名称或 ID；Enter 保留搜索，Esc 清除\n  !          查看完整操作结果\n  q / Ctrl+C 退出；未保存的修改需要确认\n\n编辑\n  ↑ ↓ / Tab  选择字段，Enter 编辑\n  单行 Enter、多行 Ctrl+S：应用字段到草稿\n  返回字段列表后 Ctrl+S：保存完整配置\n  Ctrl+U 清空字段；支持终端粘贴；API Key 默认隐藏，F2 显示\n  原始 JSON/TOML 可能包含凭据，仅在主动打开时显示\n\n");
    match app.page {
        Page::Providers => text.push_str("Codex：Enter 切换供应商，e 编辑。新增第一家供应商可能成为当前供应商。TOML 的全局设置影响所有供应商，已有 Codex 进程需要重新启动。代理推理参数使用 codexChatReasoning JSON。\nAntigravity：Enter 暂时离开 TUI 并启动 agy，退出后返回。卡片只影响本次启动。\n"),
        Page::Mcp => text.push_str("MCP 连接配置支持 stdio（command、args、env）和 HTTP/SSE（type、url、headers）。新增默认停用；Space 仅切换当前应用。编辑保留其他应用开关，删除会从所有应用移除。\n"),
        Page::Prompts => text.push_str("提示词保存为模板，Space 启用或停用并同步当前应用的提示词文件。启用新模板会保留原生内容的备份。先停用才能删除。编辑时检测数据库及原生文件冲突。\n"),
        Page::Skills | Page::Discover | Page::Repos => text.push_str("技能：f 从启用的 GitHub 仓库发现，g 管理仓库，a 从本地 ZIP 安装。安装后启用于当前应用。Space 切换当前应用，s 同步，u 更新，d 从所有应用卸载。下载失败可按 ! 查看详情。\n"),
        Page::Proxy | Page::Settings => text.push_str("代理起停复用 CLI 的 proxy enable / disable。只管理 CLI 自己启动的后台进程，不能停止桌面端的代理。用量记录和日志开关仅影响 CLI 后台代理，修改后需停止并重新启动。\n"),
        Page::Usage => text.push_str("按当前应用查询数据库已有用量及请求日志，每页 50 条，逗号/句号翻页。此入口不扫描客户端会话文件。记录开关在 CLI 设置中。\n"),
        Page::Config => text.push_str("b 使用核心 SQLite 一致性快照创建备份，位置在共享配置目录的 backups 下。\n"),
        _ => {}
    }
    text.push_str("\n耗时操作在后台执行，完成前暂停其他操作，避免写入交错。? 仍可查看帮助。\n");
    text
}
