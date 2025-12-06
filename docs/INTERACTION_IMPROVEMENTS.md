# CCMan 交互改进实现计划

## 需求概览

| # | 需求 | 优先级 | 复杂度 |
|---|------|--------|--------|
| 1 | 合并 worktree/session 为树形结构 | P3 | 高 |
| 2 | 多实例状态同步 | P2 | 中 |
| 3 | Prefix + `[` 进入 Terminal Normal | P1 | 低 |
| 4 | 修复 repo 重复创建 | P1 | 中 |
| 5 | 鼠标滚动支持 | P2 | 低 |
| 6 | 多行输入 (Shift+Enter) | P1 | 低 |

---

## 实现步骤

### Phase 1: 基础功能修复 (P1)

#### 1.1 Prefix + `[` 进入 Terminal Normal 模式

**文件**: `ccm-cli/src/tui/input.rs`

在 `handle_prefix_command_sync()` 中添加:
```rust
KeyCode::Char('[') => {
    if app.focus == Focus::Terminal && app.terminal_mode == TerminalMode::Insert {
        app.terminal_mode = TerminalMode::Normal;
        app.dirty.terminal = true;
    }
    None
}
```

**文件**: `ccm-cli/src/tui/ui.rs` - 更新帮助提示

---

#### 1.2 多行输入支持 (Shift+Enter)

**文件**: `ccm-cli/src/tui/input.rs`

修改所有输入处理函数 (`handle_input_mode_sync`, `handle_rename_session_mode_sync`, `handle_add_line_comment_mode_sync`):
```rust
match (key.code, key.modifiers) {
    (KeyCode::Enter, m) if m.contains(KeyModifiers::SHIFT) => {
        app.input_buffer.push('\n');
        None
    }
    (KeyCode::Enter, _) => Some(AsyncAction::SubmitInput),
    // ...
}
```

**文件**: `ccm-cli/src/tui/ui.rs`

更新输入框渲染以支持多行:
- 使用 `Paragraph::wrap()`
- 光标位置计算考虑多行

---

#### 1.3 修复 repo 重复创建问题

**文件**: `ccm-daemon/src/git.rs`

添加 worktree 检测函数:
```rust
impl GitOps {
    /// 检查路径是否是 worktree，如果是则返回主仓库路径
    pub fn find_main_worktree(path: &Path) -> Result<PathBuf, GitError> {
        let git_path = path.join(".git");
        if git_path.is_file() {
            // 解析 .git 文件内容: "gitdir: /path/to/main/.git/worktrees/name"
            let content = std::fs::read_to_string(&git_path)?;
            if let Some(gitdir) = content.strip_prefix("gitdir: ") {
                // 向上查找主 .git 目录
                // ...
            }
        }
        Ok(path.to_path_buf())
    }

    pub fn is_worktree(path: &Path) -> bool {
        path.join(".git").is_file()
    }
}
```

**文件**: `ccm-cli/src/main.rs`

修改自动检测逻辑:
```rust
if cwd.join(".git").exists() {
    use ccm_daemon::git::GitOps;  // 或复制逻辑到 cli

    let repo_path = if GitOps::is_worktree(&cwd) {
        GitOps::find_main_worktree(&cwd).ok()
    } else {
        Some(cwd.clone())
    };

    if let Some(path) = repo_path {
        let _ = app.client.add_repo(path.to_str().unwrap()).await;
    }
}
```

---

### Phase 2: 交互增强 (P2)

#### 2.1 鼠标滚动支持

**文件**: `ccm-cli/src/tui/input.rs`

新增鼠标处理函数:
```rust
use crossterm::event::{MouseEvent, MouseEventKind};

pub fn handle_mouse_sync(app: &mut App, mouse: MouseEvent) {
    match mouse.kind {
        MouseEventKind::ScrollUp => {
            match app.focus {
                Focus::Terminal if app.terminal_mode == TerminalMode::Normal => {
                    app.scroll_up(3);
                }
                Focus::DiffFiles => { app.diff_scroll_up(3); }
                _ => {}
            }
        }
        MouseEventKind::ScrollDown => {
            match app.focus {
                Focus::Terminal if app.terminal_mode == TerminalMode::Normal => {
                    app.scroll_down(3);
                }
                Focus::DiffFiles => { app.diff_scroll_down(3); }
                _ => {}
            }
        }
        _ => {}
    }
}
```

**文件**: `ccm-cli/src/tui/app.rs`

在主循环的 `tokio::select!` 中处理鼠标事件:
```rust
Event::Mouse(mouse) => {
    handle_mouse_sync(&mut app, mouse);
}
```

---

#### 2.2 多实例状态同步

已有的事件机制基本满足需求。需要确认:

**文件**: `ccm-proto/proto/daemon.proto`

检查是否已有 `WorktreeAdded`/`WorktreeRemoved` 事件，如没有则添加。

**文件**: `ccm-daemon/src/server.rs`

在 `create_worktree`/`remove_worktree` 中触发事件:
```rust
self.events.emit_worktree_added(info);
self.events.emit_worktree_removed(repo_id, branch);
```

**文件**: `ccm-cli/src/tui/app.rs`

在 `handle_daemon_event` 中处理新事件，刷新 UI。

---

### Phase 3: UI 重构 (P3)

#### 3.1 合并 worktree 和 session 为树形列表

**数据结构变更** (`ccm-cli/src/tui/app.rs`):

```rust
// 新增字段
pub struct App {
    pub expanded_worktrees: HashSet<usize>,     // 展开的 worktree
    pub sidebar_cursor: usize,                   // 虚拟列表光标
    pub sessions_by_worktree: HashMap<usize, Vec<SessionInfo>>,
}

// 辅助方法
impl App {
    fn sidebar_virtual_len(&self) -> usize { ... }
    fn current_sidebar_item(&self) -> SidebarItem { ... }
}

pub enum SidebarItem {
    Worktree(usize),
    Session(usize, usize),  // (wt_idx, session_idx)
}
```

**Focus 变更**:
```rust
pub enum Focus {
    Sidebar,     // 合并后的树形列表
    DiffFiles,   // 原 Sessions 区域改为文件修改列表
    Terminal,
}
```

**UI 渲染变更** (`ccm-cli/src/tui/ui.rs`):

修改 `draw_sidebar()`:
- 侧边栏不再分割为两部分
- 渲染树形结构: worktree + 缩进的 sessions
- `o` 键控制展开/折叠

原 Sessions 区域变为:
- 显示当前 worktree 的 diff 文件列表
- 复用现有 `draw_diff_inline()` 逻辑

**输入处理变更** (`ccm-cli/src/tui/input.rs`):

- 移除 `Focus::Branches` 和 `Focus::Sessions` 的单独处理
- 添加 `Focus::Sidebar` 的统一处理
- `o` 键展开/折叠 worktree
- 上下移动在虚拟列表中导航

---

## 关键文件清单

| 文件 | 涉及需求 |
|------|----------|
| `ccm-cli/src/tui/input.rs` | 1, 3, 5, 6 |
| `ccm-cli/src/tui/app.rs` | 1, 2, 5 |
| `ccm-cli/src/tui/ui.rs` | 1, 3, 6 |
| `ccm-cli/src/main.rs` | 4 |
| `ccm-daemon/src/git.rs` | 4 |
| `ccm-proto/proto/daemon.proto` | 2 |
| `ccm-daemon/src/server.rs` | 2 |
| `ccm-daemon/src/events.rs` | 2 |

---

## 实施顺序

1. **Phase 1** - 基础修复 (可并行)
   - [ ] 1.1 Prefix + `[`
   - [ ] 1.2 多行输入
   - [ ] 1.3 修复 repo 创建

2. **Phase 2** - 交互增强
   - [ ] 2.1 鼠标滚动
   - [ ] 2.2 多实例同步

3. **Phase 3** - UI 重构
   - [ ] 3.1 树形列表合并 (依赖 Phase 1/2 完成)
