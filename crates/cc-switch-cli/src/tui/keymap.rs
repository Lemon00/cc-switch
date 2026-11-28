use super::data::Page;
use cc_switch_core::AppType;
use crossterm::event::KeyCode;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Intent {
    New,
    Edit,
    Copy,
    Delete,
    Activate,
    Import,
    Refresh,
    Search,
    Discover,
    Repos,
    Zip,
    Sync,
    Update,
    Backup,
    StartProxy,
    StopProxy,
    PreviousPage,
    NextPage,
    Providers,
}

pub(super) struct Binding {
    pub key: KeyCode,
    pub label: &'static str,
    pub hint: &'static str,
    pub intent: Intent,
}

pub(super) fn bindings(page: Page, app: &AppType) -> Vec<Binding> {
    use Intent::*;
    let mut items = Vec::new();
    let mut add = |key, label, hint, intent| {
        items.push(Binding {
            key,
            label,
            hint,
            intent,
        })
    };
    match page {
        Page::Home => {
            add(KeyCode::Char('p'), "p", "供应商", Providers);
        }
        Page::Providers => {
            add(
                KeyCode::Enter,
                "Enter",
                if *app == AppType::Agy {
                    "启动"
                } else {
                    "切换"
                },
                Activate,
            );
            add(KeyCode::Char('e'), "e", "编辑", Edit);
            add(KeyCode::Char('a'), "a", "新增", New);
            add(KeyCode::Char('c'), "c", "复制", Copy);
            add(KeyCode::Char('d'), "d", "删除", Delete);
            add(KeyCode::Char('i'), "i", "导入本机", Import);
        }
        Page::Mcp | Page::Prompts => {
            add(KeyCode::Enter, "Enter", "编辑", Edit);
            add(KeyCode::Char(' '), "Space", "启用/停用", Activate);
            add(KeyCode::Char('a'), "a", "新增", New);
            add(KeyCode::Char('d'), "d", "删除", Delete);
            add(KeyCode::Char('i'), "i", "导入本机", Import);
        }
        Page::Skills => {
            add(KeyCode::Enter, "Enter", "详情", Edit);
            add(KeyCode::Char(' '), "Space", "启用/停用", Activate);
            add(KeyCode::Char('f'), "f", "发现", Discover);
            add(KeyCode::Char('g'), "g", "仓库", Repos);
            add(KeyCode::Char('a'), "a", "ZIP 安装", Zip);
            add(KeyCode::Char('s'), "s", "同步", Sync);
            add(KeyCode::Char('u'), "u", "更新", Update);
            add(KeyCode::Char('d'), "d", "卸载", Delete);
        }
        Page::Discover => {
            add(KeyCode::Enter, "Enter", "安装", Activate);
            add(KeyCode::Char('g'), "g", "仓库", Repos);
        }
        Page::Repos => {
            add(KeyCode::Enter, "Enter", "编辑", Edit);
            add(KeyCode::Char('a'), "a", "新增", New);
            add(KeyCode::Char('d'), "d", "移除", Delete);
            add(KeyCode::Char('f'), "f", "发现技能", Discover);
        }
        Page::Proxy if *app == AppType::Codex => {
            add(KeyCode::Char('s'), "s", "启动代理", StartProxy);
            add(KeyCode::Char('x'), "x", "停止代理", StopProxy);
        }
        Page::Usage => {
            add(KeyCode::Enter, "Enter", "请求详情", Edit);
            add(KeyCode::Char(','), ",", "上一页", PreviousPage);
            add(KeyCode::Char('.'), ".", "下一页", NextPage);
        }
        Page::Config => {
            add(KeyCode::Char('b'), "b", "创建备份", Backup);
        }
        _ => {}
    }
    if page.searchable() {
        add(KeyCode::Char('/'), "/", "搜索", Search);
    }
    add(KeyCode::Char('r'), "r", "刷新", Refresh);
    items
}
