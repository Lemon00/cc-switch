use super::*;

pub(super) fn render(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    match app.page {
        Page::Home => home(frame, area, app, theme),
        Page::Proxy => frame.render_widget(
            Paragraph::new(
                app.data
                    .proxy
                    .lines()
                    .map(|s| Line::raw(clean(s)))
                    .collect::<Vec<_>>(),
            )
            .wrap(Wrap { trim: false }),
            area,
        ),
        Page::Config => config(frame, area, theme),
        Page::Settings => settings(frame, area, app, theme),
        _ => listing(frame, area, app, theme),
    }
}

fn home(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    let layout = Layout::vertical([
        Constraint::Length(if area.height >= 18 { 5 } else { 3 }),
        Constraint::Length(6),
        Constraint::Min(1),
    ])
    .split(area);
    let cards = Layout::horizontal([Constraint::Percentage(25); 4]).split(layout[0]);
    let counts = [
        ("供应商", app.providers.len().to_string()),
        (
            "MCP 启用",
            format!(
                "{} / {}",
                app.data
                    .mcp
                    .iter()
                    .filter(|s| s.apps.is_enabled_for(&app.app()))
                    .count(),
                app.data.mcp.len()
            ),
        ),
        ("提示词", app.data.prompts.len().to_string()),
        (
            "技能启用",
            format!(
                "{} / {}",
                app.data
                    .skills
                    .iter()
                    .filter(|s| s.apps.is_enabled_for(&app.app()))
                    .count(),
                app.data.skills.len()
            ),
        ),
    ];
    for (i, (title, value)) in counts.iter().enumerate() {
        frame.render_widget(
            Paragraph::new(value.as_str())
                .alignment(Alignment::Center)
                .style(
                    Style::default()
                        .fg(theme.accent)
                        .add_modifier(Modifier::BOLD),
                )
                .block(panel(*title, theme, false)),
            cards[i],
        );
    }
    let name = app
        .providers
        .iter()
        .find(|p| p.id == app.current)
        .map(|p| p.name.as_str())
        .unwrap_or("未选择");
    let prompt = app
        .data
        .prompts
        .iter()
        .find(|p| p.enabled)
        .map(|p| p.name.as_str())
        .unwrap_or("未启用");
    let rows = vec![
        Line::from(vec![
            Span::styled("当前供应商    ", Style::default().fg(theme.muted)),
            Span::raw(clean(name)),
        ]),
        Line::from(vec![
            Span::styled("当前提示词    ", Style::default().fg(theme.muted)),
            Span::raw(clean(prompt)),
        ]),
        Line::from(vec![
            Span::styled("配置目录      ", Style::default().fg(theme.muted)),
            Span::raw(super::super::super::app_dir().display().to_string()),
        ]),
    ];
    frame.render_widget(
        Paragraph::new(rows).block(panel("工作区", theme, false)),
        layout[1],
    );
    if layout[2].height >= 4 {
        frame.render_widget(
            Paragraph::new(
                app.data
                    .proxy
                    .lines()
                    .map(|s| Line::raw(clean(s)))
                    .collect::<Vec<_>>(),
            )
            .wrap(Wrap { trim: false })
            .block(panel("连接状态", theme, false)),
            layout[2],
        );
    }
}

fn config(frame: &mut Frame, area: Rect, theme: &Theme) {
    let dir = super::super::super::app_dir();
    let rows = vec![
        vec!["配置目录".into(), dir.display().to_string()],
        vec![
            "数据库".into(),
            dir.join("cc-switch.db").display().to_string(),
        ],
        vec![
            "CLI 设置".into(),
            dir.join("cli.json").display().to_string(),
        ],
        vec![
            "应用白名单".into(),
            dir.join("frontend-apps.json").display().to_string(),
        ],
        vec!["备份目录".into(), dir.join("backups").display().to_string()],
    ];
    table(
        frame,
        area,
        &["项目", "路径"],
        rows,
        usize::MAX,
        vec![Constraint::Length(12), Constraint::Min(10)],
        theme,
        false,
    );
}

fn settings(frame: &mut Frame, area: Rect, app: &App, theme: &Theme) {
    if let Some(settings) = &app.settings {
        let rows = LABELS
            .iter()
            .enumerate()
            .map(|(i, label)| {
                vec![
                    label.to_string(),
                    if settings.values[i] {
                        "● 开启".into()
                    } else {
                        "○ 关闭".into()
                    },
                ]
            })
            .collect();
        table(
            frame,
            area,
            &["设置", "状态"],
            rows,
            settings.selected,
            vec![Constraint::Percentage(65), Constraint::Min(8)],
            theme,
            !app.nav_focus,
        );
    } else {
        empty(
            frame,
            area,
            "无法读取 CLI 设置",
            "修正 cli.json 后按 r 重试；! 查看错误详情",
            theme,
        );
    }
}

fn listing(frame: &mut Frame, mut area: Rect, app: &App, theme: &Theme) {
    if app.page == Page::Providers && area.height >= 17 {
        if let Some(index) = app.selected_index() {
            let provider = &app.providers[index];
            let (endpoint, model) = if app.app() == cc_switch_core::AppType::Codex {
                let config = provider
                    .settings_config
                    .get("config")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();
                // Only parse a bounded passive summary; full configuration stays in the editor.
                let doc = (config.len() <= 65536)
                    .then(|| config.parse::<toml_edit::DocumentMut>().ok())
                    .flatten();
                let route = doc
                    .as_ref()
                    .and_then(|d| d.get("model_provider"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("openai");
                let endpoint = doc
                    .as_ref()
                    .and_then(|d| d.get("model_providers"))
                    .and_then(|p| p.get(route))
                    .and_then(|p| p.get("base_url"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("默认 / 托管地址")
                    .to_owned();
                let model = doc
                    .as_ref()
                    .and_then(|d| d.get("model"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("默认模型")
                    .to_owned();
                (endpoint, model)
            } else {
                (
                    provider
                        .settings_config
                        .pointer("/env/GOOGLE_GEMINI_BASE_URL")
                        .and_then(|v| v.as_str())
                        .unwrap_or("官方登录")
                        .to_owned(),
                    provider
                        .settings_config
                        .pointer("/config/model/name")
                        .or_else(|| provider.settings_config.pointer("/env/GEMINI_MODEL"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("默认模型")
                        .to_owned(),
                )
            };
            let chunks = Layout::vertical([Constraint::Min(7), Constraint::Length(6)]).split(area);
            let lines = vec![
                Line::from(vec![
                    Span::styled("API 地址  ", Style::default().fg(theme.muted)),
                    Span::raw(clean(&endpoint)),
                ]),
                Line::from(vec![
                    Span::styled("模型      ", Style::default().fg(theme.muted)),
                    Span::raw(clean(&model)),
                ]),
                Line::from(vec![
                    Span::styled("备注      ", Style::default().fg(theme.muted)),
                    Span::raw(clean(provider.notes.as_deref().unwrap_or("—"))),
                ]),
            ];
            frame.render_widget(
                Paragraph::new(lines).block(panel("所选供应商", theme, false)),
                chunks[1],
            );
            area = chunks[0];
        }
    }
    if app.page == Page::Usage {
        let chunks = Layout::vertical([
            Constraint::Length(3),
            Constraint::Min(1),
            Constraint::Length(1),
        ])
        .split(area);
        if let Some(summary) = &app.data.usage {
            frame.render_widget(
                Paragraph::new(format!(
                    "请求 {}   Tokens {}   费用 ${}\n成功率 {:.1}%   缓存命中 {:.1}%",
                    summary.total_requests,
                    summary.real_total_tokens,
                    summary.total_cost,
                    summary.success_rate,
                    summary.cache_hit_rate * 100.0
                ))
                .style(Style::default().fg(theme.accent)),
                chunks[0],
            );
        }
        let total = app.data.logs.as_ref().map_or(0, |l| l.total);
        frame.render_widget(
            Paragraph::new(format!(
                "第 {} / {} 页 · 共 {} 条 · 每页 50 条",
                app.log_page + 1,
                total.saturating_sub(1) / 50 + 1,
                total
            ))
            .style(Style::default().fg(theme.muted)),
            chunks[2],
        );
        area = chunks[1];
    }
    let visible = app.visible();
    if visible.is_empty() {
        let subtitle = if !app.filter.is_empty() {
            "Esc 清除搜索"
        } else {
            match app.page {
                Page::Providers | Page::Mcp | Page::Prompts => "a 新增 · i 从本机导入",
                Page::Skills => "f 发现技能 · a 从 ZIP 安装",
                Page::Discover => "g 管理仓库 · r 重新发现",
                Page::Repos => "a 添加 GitHub 仓库",
                Page::Usage => "CLI 设置中可开启代理用量记录",
                _ => "r 刷新",
            }
        };
        empty(
            frame,
            area,
            if app.busy.is_some() {
                "正在加载…"
            } else if !app.filter.is_empty() {
                "没有匹配的项目"
            } else {
                "暂无数据"
            },
            subtitle,
            theme,
        );
        return;
    }
    let enabled = |v| {
        if v {
            "● 启用".to_string()
        } else {
            "○ 停用".to_string()
        }
    };
    let mut rows = vec![];
    let headers: &[&str] = match app.page {
        Page::Providers => {
            for i in visible {
                let p = &app.providers[i];
                rows.push(vec![
                    if p.id == app.current {
                        "● 当前".into()
                    } else {
                        "○".into()
                    },
                    p.name.clone(),
                    p.id.clone(),
                ]);
            }
            &["状态", "供应商", "ID"]
        }
        Page::Mcp => {
            for i in visible {
                let p = &app.data.mcp[i];
                rows.push(vec![
                    enabled(p.apps.is_enabled_for(&app.app())),
                    p.name.clone(),
                    p.server
                        .get("type")
                        .and_then(|v| v.as_str())
                        .unwrap_or(if p.server.get("url").is_some() {
                            "http"
                        } else {
                            "stdio"
                        })
                        .into(),
                ]);
            }
            &["当前应用", "服务器", "传输"]
        }
        Page::Prompts => {
            for i in visible {
                let p = &app.data.prompts[i];
                rows.push(vec![
                    enabled(p.enabled),
                    p.name.clone(),
                    p.description
                        .clone()
                        .unwrap_or_else(|| format!("{} 行", p.content.lines().count())),
                ]);
            }
            &["状态", "提示词", "描述"]
        }
        Page::Skills => {
            for i in visible {
                let p = &app.data.skills[i];
                rows.push(vec![
                    enabled(p.apps.is_enabled_for(&app.app())),
                    p.name.clone(),
                    p.description.clone().unwrap_or_else(|| p.directory.clone()),
                ]);
            }
            &["当前应用", "技能", "描述"]
        }
        Page::Discover => {
            for i in visible {
                let p = &app.discovered[i];
                rows.push(vec![
                    if app.data.skills.iter().any(|s| s.id == p.key) {
                        "已安装".into()
                    } else {
                        "可安装".into()
                    },
                    p.name.clone(),
                    format!("{}/{}", p.repo_owner, p.repo_name),
                ]);
            }
            &["状态", "技能", "来源"]
        }
        Page::Repos => {
            for i in visible {
                let p = &app.data.repos[i];
                rows.push(vec![
                    enabled(p.enabled),
                    format!("{}/{}", p.owner, p.name),
                    p.branch.clone(),
                ]);
            }
            &["状态", "GitHub 仓库", "分支"]
        }
        Page::Usage => {
            for i in visible {
                let p = &app.data.logs.as_ref().unwrap().data[i];
                rows.push(vec![
                    p.status_code.to_string(),
                    p.model.clone(),
                    format!("{} ms  ${}", p.latency_ms, p.total_cost_usd),
                ]);
            }
            &["状态码", "模型", "耗时 / 费用"]
        }
        _ => return,
    };
    table(
        frame,
        area,
        headers,
        rows,
        app.selected,
        vec![
            Constraint::Length(9),
            Constraint::Percentage(42),
            Constraint::Min(5),
        ],
        theme,
        !app.nav_focus,
    );
}
