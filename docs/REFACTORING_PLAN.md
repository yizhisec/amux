# CCM 代码重构计划

## 概述

本文档描述了 CCM (Claude Code Manager) 项目的代码重构计划。主要目标是改善代码组织、减少重复、提高可维护性和测试覆盖率。

### 当前问题

| 文件 | 行数 | 问题 |
|------|------|------|
| `amux-cli/src/tui/app.rs` | 3386 | App 结构体 40+ 字段，61 个方法 |
| `amux-cli/src/tui/ui.rs` | 2088 | 35 个渲染函数混在一个文件 |
| `amux-cli/src/tui/input.rs` | 1356 | 29 个输入处理函数 |
| `amux-daemon/src/server.rs` | 1310 | 31 个 gRPC 方法在一个 impl |

---

## Phase 1: 提取状态类型和枚举 (低风险)

**目标**: 将类型定义从实现逻辑中分离出来

### 1.1 创建 State 模块

**新文件**: `amux-cli/src/tui/state.rs`

从 `app.rs` (32-196行) 提取以下类型：

```rust
// 焦点枚举
pub enum Focus {
    Branches,
    Sessions,
    Sidebar,
    GitStatus,
    Terminal,
    DiffFiles,
}

// 侧边栏项
pub enum SidebarItem {
    Worktree(usize),
    Session { worktree_idx: usize, session: SessionInfo },
}

// Git 区域
pub enum GitSection {
    Staged,
    Unstaged,
    Untracked,
}

// Git 状态文件
pub struct GitStatusFile {
    pub path: String,
    pub status: String,
    pub staged: bool,
}

// Git 面板项
pub enum GitPanelItem {
    SectionHeader(GitSection),
    File(GitStatusFile),
}

// 右面板视图
pub enum RightPanelView {
    Terminal,
    Diff,
}

// Diff 项
pub enum DiffItem {
    File(usize),
    Line { file_idx: usize, line_idx: usize },
}

// 删除目标
pub enum DeleteTarget {
    Session(String),
}

// 输入模式
pub enum InputMode {
    Normal,
    NewBranch,
    AddWorktree { base_branch: Option<String> },
    RenameSession { session_id: String },
    ConfirmDelete(DeleteTarget),
    ConfirmDeleteBranch { branch: String, worktree_path: Option<String> },
    ConfirmDeleteWorktreeSessions { worktree_path: String, sessions: Vec<SessionInfo> },
    AddLineComment { file_path: String, line_number: i32, line_type: i32 },
    EditLineComment { comment: LineCommentInfo },
    TodoPopup,
    AddTodo { parent_id: Option<String> },
    EditTodo { todo_id: String },
    EditTodoDescription { todo_id: String },
    ConfirmDeleteTodo { todo_id: String },
}

// 终端模式
pub enum TerminalMode {
    Normal,
    Insert,
}

// 前缀模式
pub enum PrefixMode {
    None,
    CtrlS,
}

// 脏标志
pub struct DirtyFlags {
    pub repos: bool,
    pub branches: bool,
    pub sessions: bool,
    pub terminal: bool,
    pub diff: bool,
    pub git_status: bool,
    pub line_comments: bool,
    pub todos: bool,
}

// 异步操作
pub enum AsyncAction {
    RefreshRepos,
    RefreshBranches,
    RefreshSessions,
    RefreshWorktreeSessions,
    CreateSession,
    DestroySession(String),
    AttachSession(String),
    DetachSession,
    RenameSession { session_id: String, new_name: String },
    CreateWorktree { branch_name: String, base_branch: Option<String> },
    DeleteBranch { branch: String, worktree_path: Option<String> },
    DeleteWorktreeAndSessions { worktree_path: String },
    RefreshDiff,
    LoadDiffLines(usize),
    RefreshGitStatus,
    StageFile(String),
    UnstageFile(String),
    StageAll,
    UnstageAll,
    CreateLineComment { file_path: String, line_number: i32, line_type: i32, comment: String },
    UpdateLineComment { comment_id: String, comment: String },
    DeleteLineComment(String),
    RefreshLineComments,
    RefreshTodos,
    CreateTodo { title: String, parent_id: Option<String> },
    UpdateTodoTitle { todo_id: String, title: String },
    UpdateTodoDescription { todo_id: String, description: String },
    ToggleTodo(String),
    DeleteTodo(String),
}

// 终端流
pub struct TerminalStream {
    pub session_id: String,
    pub input_tx: mpsc::Sender<AttachInput>,
    pub output_rx: mpsc::Receiver<Vec<u8>>,
}
```

### 1.2 更新 mod.rs

```rust
// amux-cli/src/tui/mod.rs
mod app;
pub mod highlight;
mod input;
mod state;  // 新增
mod ui;

pub use app::{run_with_client, App};
pub use state::*;  // 重新导出
```

### 1.3 验证

```bash
cargo check --workspace
cargo clippy --workspace
```

---

## Phase 2: 拆分 UI 渲染函数 (中等风险)

**目标**: 将 `ui.rs` 按功能域拆分为多个模块

### 2.1 目录结构

```
amux-cli/src/tui/ui/
    mod.rs          # 主 draw() + 重新导出
    tab_bar.rs      # draw_tab_bar, draw_status_bar
    sidebar.rs      # draw_sidebar, draw_sidebar_tree, draw_worktrees, draw_sessions
    git_panel.rs    # draw_git_status_panel
    terminal.rs     # draw_terminal, draw_terminal_fullscreen
    diff.rs         # draw_diff_view, draw_diff_inline, word diff helpers
    overlays.rs     # 所有弹窗/对话框渲染
    helpers.rs      # compute_lcs, tint_color, find_syntax_style_for_range
```

### 2.2 提取详情

**ui/sidebar.rs** (从原 530-805 行提取):
- `draw_sidebar()`
- `draw_sidebar_tree()`
- `draw_worktrees()`
- `draw_sessions()`

**ui/git_panel.rs** (从原 807-941 行提取):
- `draw_git_status_panel()`

**ui/terminal.rs** (从原 943-1012 行提取):
- `draw_terminal()`
- `draw_terminal_fullscreen()`

**ui/diff.rs** (从原 23-352, 1014-1341 行提取):
- `DiffToken` enum
- `compute_word_diff()`
- `compute_lcs()`
- `find_paired_deletion()`
- `find_paired_addition()`
- `render_word_diff_line()`
- `find_syntax_style_for_range()`
- `tint_color()`
- `draw_diff_view()`
- `draw_diff_inline()`
- `draw_diff_fullscreen()`

**ui/overlays.rs** (从原 1343-2088 行提取):
- `draw_input_overlay()`
- `draw_rename_session_overlay()`
- `draw_confirm_delete_overlay()`
- `draw_confirm_delete_branch_overlay()`
- `draw_confirm_delete_worktree_sessions_overlay()`
- `draw_add_worktree_overlay()`
- `draw_add_line_comment_overlay()`
- `draw_edit_line_comment_overlay()`
- `draw_todo_popup()`
- `draw_add_todo_overlay()`
- `draw_edit_todo_overlay()`
- `draw_edit_todo_description_overlay()`
- `draw_confirm_delete_todo_overlay()`

### 2.3 测试

为每个 UI 模块添加基础测试：
- 测试渲染函数不会 panic
- 测试边界条件（空列表、极长文本等）

---

## Phase 3: 拆分输入处理函数 (中等风险)

**目标**: 按上下文/模式组织输入处理

### 3.1 目录结构

```
amux-cli/src/tui/input/
    mod.rs              # handle_input_sync() 主分发器
    navigation.rs       # handle_navigation_input_sync
    terminal.rs         # handle_insert_mode_sync, handle_terminal_normal_mode_sync
    prefix.rs           # handle_prefix_command_sync, is_prefix_key
    dialogs.rs          # 所有确认/输入模式处理器
    diff.rs             # handle_diff_files_mode_sync
    git_status.rs       # handle_git_status_input_sync
    todo.rs             # 所有 TODO 相关处理器
    mouse.rs            # handle_mouse_sync
    key_convert.rs      # key_to_bytes, is_text_input_mode
```

### 3.2 提取详情

**input/dialogs.rs** (从原 267-458 行提取):
- `handle_confirm_delete_sync()`
- `handle_confirm_delete_branch_sync()`
- `handle_confirm_delete_worktree_sessions_sync()`
- `handle_add_worktree_mode_sync()`
- `handle_rename_session_mode_sync()`
- `handle_add_line_comment_mode_sync()`
- `handle_edit_line_comment_mode_sync()`
- `handle_input_mode_sync()` (NewBranch)

**input/todo.rs** (从原 1064-1356 行提取):
- `handle_todo_popup_sync()`
- `handle_add_todo_mode_sync()`
- `handle_edit_todo_mode_sync()`
- `handle_edit_todo_description_mode_sync()`
- `handle_confirm_delete_todo_sync()`

### 3.3 测试

为每个输入模块添加测试：
- 测试按键到动作的映射
- 测试边界情况（无效输入、空状态等）

---

## Phase 4: 分解 App 结构体 (高风险 - 谨慎处理)

**目标**: 将 App 的字段按功能分组提取到独立的状态类型

### 4.1 提取 TerminalState

**新文件**: `amux-cli/src/tui/terminal_state.rs`

```rust
pub struct TerminalState {
    pub parser: Arc<Mutex<vt100::Parser>>,
    pub session_parsers: HashMap<String, Arc<Mutex<vt100::Parser>>>,
    pub active_session_id: Option<String>,
    pub is_interactive: bool,
    pub stream: Option<TerminalStream>,
    pub mode: TerminalMode,
    pub scroll_offset: usize,
    pub fullscreen: bool,
    pub last_hash: u64,
    pub cached_lines: Vec<ratatui::text::Line<'static>>,
    pub cached_size: (u16, u16),
    pub cols: Option<u16>,
    pub rows: Option<u16>,
}

impl TerminalState {
    pub fn new() -> Self { ... }
    pub fn scroll_up(&mut self, lines: usize) { ... }
    pub fn scroll_down(&mut self, lines: usize) { ... }
    pub fn scroll_to_top(&mut self) { ... }
    pub fn scroll_to_bottom(&mut self) { ... }
    pub fn update_hash(&mut self) -> bool { ... }
    pub fn get_lines(&self, height: u16, width: u16) -> Vec<Line<'static>> { ... }
}
```

### 4.2 提取 DiffState

**新文件**: `amux-cli/src/tui/diff_state.rs`

```rust
pub struct DiffState {
    pub files: Vec<DiffFileInfo>,
    pub expanded: HashSet<usize>,
    pub file_lines: HashMap<usize, Vec<DiffLine>>,
    pub cursor: usize,
    pub scroll_offset: usize,
    pub fullscreen: bool,
}

impl DiffState {
    pub fn new() -> Self { ... }
    pub fn virtual_list_len(&self) -> usize { ... }
    pub fn current_item(&self) -> DiffItem { ... }
    pub fn move_up(&mut self) { ... }
    pub fn move_down(&mut self) { ... }
    pub fn prev_file(&mut self) { ... }
    pub fn next_file(&mut self) { ... }
    pub fn toggle_expand(&mut self) -> Option<AsyncAction> { ... }
}
```

### 4.3 提取 GitState

**新文件**: `amux-cli/src/tui/git_state.rs`

```rust
pub struct GitState {
    pub files: Vec<GitStatusFile>,
    pub cursor: usize,
    pub expanded_sections: HashSet<GitSection>,
    pub scroll_offset: usize,
}

impl GitState {
    pub fn new() -> Self { ... }
    pub fn virtual_len(&self) -> usize { ... }
    pub fn current_item(&self) -> GitPanelItem { ... }
    pub fn move_up(&mut self) { ... }
    pub fn move_down(&mut self) { ... }
    pub fn toggle_section(&mut self) { ... }
    pub fn current_file_path(&self) -> Option<String> { ... }
    pub fn is_current_staged(&self) -> bool { ... }
}
```

### 4.4 提取 SidebarState

**新文件**: `amux-cli/src/tui/sidebar_state.rs`

```rust
pub struct SidebarState {
    pub tree_view_enabled: bool,
    pub expanded_worktrees: HashSet<usize>,
    pub cursor: usize,
    pub sessions_by_worktree: HashMap<usize, Vec<SessionInfo>>,
    pub repo_idx: usize,
    pub branch_idx: usize,
    pub session_idx: usize,
}

impl SidebarState {
    pub fn new() -> Self { ... }
    pub fn virtual_len(&self, worktrees: &[WorktreeInfo]) -> usize { ... }
    pub fn current_item(&self, worktrees: &[WorktreeInfo]) -> SidebarItem { ... }
    pub fn move_up(&mut self) -> Option<AsyncAction> { ... }
    pub fn move_down(&mut self, worktrees: &[WorktreeInfo]) -> Option<AsyncAction> { ... }
    pub fn toggle_expand(&mut self) -> Option<AsyncAction> { ... }
}
```

### 4.5 提取 TodoState

**新文件**: `amux-cli/src/tui/todo_state.rs`

```rust
pub struct TodoState {
    pub items: Vec<TodoItem>,
    pub cursor: usize,
    pub expanded: HashSet<String>,
    pub scroll_offset: usize,
    pub show_completed: bool,
    pub display_order: Vec<usize>,
}

impl TodoState {
    pub fn new() -> Self { ... }
    pub fn rebuild_display_order(&mut self) { ... }
    // 导航方法...
}
```

### 4.6 简化后的 App 结构体

```rust
pub struct App {
    pub client: Client,

    // 数据
    pub repos: Vec<RepoInfo>,
    pub worktrees: Vec<WorktreeInfo>,
    pub available_branches: Vec<WorktreeInfo>,
    pub sessions: Vec<SessionInfo>,
    pub line_comments: Vec<LineCommentInfo>,

    // 提取的状态组件
    pub terminal: TerminalState,
    pub diff: DiffState,
    pub git: GitState,
    pub sidebar: SidebarState,
    pub todo: TodoState,

    // UI 状态
    pub focus: Focus,
    pub right_panel_view: RightPanelView,
    pub should_quit: bool,
    pub error_message: Option<String>,
    pub status_message: Option<String>,
    pub input_mode: InputMode,
    pub input_buffer: String,
    pub prefix_mode: PrefixMode,
    pub dirty: DirtyFlags,

    // 事件
    pub event_rx: Option<mpsc::Receiver<DaemonEvent>>,
}
```

### 4.7 测试

为每个状态模块添加单元测试：
- 导航逻辑测试
- 边界检查测试
- 状态转换测试

---

## Phase 5: 重构 Daemon Server (中等风险)

**目标**: 按域组织 gRPC 处理器

### 5.1 目录结构

```
amux-daemon/src/handlers/
    mod.rs              # CcmDaemonService 定义
    repo.rs             # add_repo, list_repos, remove_repo
    worktree.rs         # list_worktrees, create_worktree, remove_worktree, delete_branch
    session.rs          # CRUD + attach_session
    events.rs           # subscribe_events
    diff.rs             # get_diff_files, get_file_diff
    comments.rs         # create/update/delete/list_line_comment
    git_status.rs       # get_git_status, stage/unstage 操作
    todo.rs             # TODO 项的 CRUD
```

### 5.2 基于 Trait 的组织

```rust
// handlers/mod.rs
pub struct CcmDaemonService {
    state: SharedState,
    events: EventBroadcaster,
}

// 每个处理器模块实现部分 trait
// handlers/repo.rs
impl CcmDaemonService {
    pub(crate) async fn handle_add_repo(&self, req: AddRepoRequest) -> Result<RepoInfo, Status> { ... }
    pub(crate) async fn handle_list_repos(&self) -> Result<ListReposResponse, Status> { ... }
    pub(crate) async fn handle_remove_repo(&self, req: RemoveRepoRequest) -> Result<(), Status> { ... }
}

// 主 impl 委托给处理器
#[tonic::async_trait]
impl CcmDaemon for CcmDaemonService {
    async fn add_repo(&self, request: Request<AddRepoRequest>) -> Result<Response<RepoInfo>, Status> {
        self.handle_add_repo(request.into_inner()).await.map(Response::new)
    }
    // ...
}
```

### 5.3 测试

为每个处理器模块添加集成测试

---

## Phase 6: 解决代码重复 (低风险)

### 6.1 提取导航 Trait

**新文件**: `amux-cli/src/tui/navigation.rs`

```rust
/// 虚拟列表导航的通用 trait
pub trait VirtualList {
    fn virtual_len(&self) -> usize;
    fn cursor(&self) -> usize;
    fn set_cursor(&mut self, pos: usize);

    fn move_up(&mut self) -> bool {
        if self.cursor() > 0 {
            self.set_cursor(self.cursor() - 1);
            true
        } else {
            false
        }
    }

    fn move_down(&mut self) -> bool {
        let max = self.virtual_len().saturating_sub(1);
        if self.cursor() < max {
            self.set_cursor(self.cursor() + 1);
            true
        } else {
            false
        }
    }
}

// 为 SidebarState, GitState, DiffState, TodoState 实现
```

### 6.2 提取文本输入处理器

**新文件**: `amux-cli/src/tui/input/text_input.rs`

```rust
/// 通用文本输入处理（所有文本输入模式共享）
pub fn handle_text_input(
    key: KeyEvent,
    buffer: &mut String,
    on_submit: impl FnOnce(String) -> Option<AsyncAction>,
    on_cancel: impl FnOnce(),
) -> Option<AsyncAction> {
    match (key.code, key.modifiers) {
        (KeyCode::Enter, m) if m.contains(KeyModifiers::SHIFT) => {
            buffer.push('\n');
            None
        }
        (KeyCode::Enter, _) => on_submit(std::mem::take(buffer)),
        (KeyCode::Esc, _) => {
            on_cancel();
            None
        }
        (KeyCode::Backspace, _) => {
            buffer.pop();
            None
        }
        (KeyCode::Char(c), _) => {
            buffer.push(c);
            None
        }
        _ => None,
    }
}
```

### 6.3 提取确认对话框处理器

```rust
/// 通用确认对话框处理
pub fn handle_confirmation(
    key: KeyEvent,
    on_confirm: impl FnOnce() -> Option<AsyncAction>,
    on_cancel: impl FnOnce(),
) -> Option<AsyncAction> {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => on_confirm(),
        KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
            on_cancel();
            None
        }
        _ => None,
    }
}
```

---

## Phase 7: 清理死代码 (低风险)

### 7.1 移除 `#[allow(dead_code)]` 项

重构完成后，审查并移除：
- `DirtyFlags` 方法（如未使用）
- `current_list_len()`, `current_idx()` 在 App 中（如未使用）
- `toggle_focus()`（如未使用）
- `enter_interactive()`, `exit_interactive()`（标记为 deprecated）
- `poll_terminal_output()`, `poll_events()`（如未使用）
- `InputMode::NewBranch`（根据注释已 deprecated）

### 7.2 移除废弃代码

- 从 `InputMode` 移除 `NewBranch` 变体
- 清理 `enter_interactive_mode` 引用

---

## Phase 8: 添加测试

### 8.1 状态组件的单元测试

**新文件**: `amux-cli/src/tui/terminal_state_test.rs`

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scroll_up_down() { ... }

    #[test]
    fn test_hash_change_detection() { ... }
}
```

类似地为 `DiffState`, `GitState`, `SidebarState`, `TodoState` 添加测试。

### 8.2 输入处理的集成测试

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_navigation_input() { ... }

    #[test]
    fn test_text_input_modes() { ... }

    #[test]
    fn test_confirmation_dialogs() { ... }
}
```

---

## 实施顺序和依赖关系

```
Phase 1 (类型提取) ─────┐
                        │
Phase 2 (UI 拆分) ──────┤
                        │
Phase 3 (Input 拆分) ───┼──► Phase 4 (App 分解)
                        │
Phase 5 (Server 拆分) ──┘

                        ▼

Phase 6 (去重) ──► Phase 7 (清理) ──► Phase 8 (测试)
```

### 建议执行时间

| 阶段 | 预估工作量 | 风险级别 | 依赖 |
|------|-----------|---------|------|
| 1    | 2-3 小时  | 低      | 无   |
| 2    | 4-6 小时  | 中      | 阶段 1 |
| 3    | 3-4 小时  | 中      | 阶段 1 |
| 4    | 6-8 小时  | 高      | 阶段 1-3 |
| 5    | 4-5 小时  | 中      | 无   |
| 6    | 2-3 小时  | 低      | 阶段 4 |
| 7    | 1-2 小时  | 低      | 阶段 4,6 |
| 8    | 4-6 小时  | 低      | 全部 |

**总计: 约 26-37 小时**

---

## 验证策略

每个阶段完成后：

1. **编译检查**: `cargo check --workspace`
2. **Lint 检查**: `cargo clippy --workspace`
3. **测试套件**: `cargo test --workspace`
4. **手动测试**: 启动 TUI，验证所有功能正常
5. **Git 提交**: 每个阶段一个原子提交，便于回滚

---

## 关键文件列表

### 需要修改的文件

| 文件路径 | 操作 |
|---------|------|
| `amux-cli/src/tui/app.rs` | 提取类型、分解结构体 |
| `amux-cli/src/tui/ui.rs` | 拆分为多个模块 |
| `amux-cli/src/tui/input.rs` | 拆分为多个模块 |
| `amux-cli/src/tui/mod.rs` | 更新模块导出 |
| `amux-daemon/src/server.rs` | 拆分为处理器模块 |
| `amux-daemon/src/main.rs` | 更新引用 |

### 需要创建的新文件

| 文件路径 | 描述 |
|---------|------|
| `amux-cli/src/tui/state.rs` | 类型和枚举定义 |
| `amux-cli/src/tui/terminal_state.rs` | 终端状态管理 |
| `amux-cli/src/tui/diff_state.rs` | Diff 状态管理 |
| `amux-cli/src/tui/git_state.rs` | Git 状态管理 |
| `amux-cli/src/tui/sidebar_state.rs` | 侧边栏状态管理 |
| `amux-cli/src/tui/todo_state.rs` | TODO 状态管理 |
| `amux-cli/src/tui/navigation.rs` | 导航 trait |
| `amux-cli/src/tui/ui/mod.rs` | UI 主模块 |
| `amux-cli/src/tui/ui/tab_bar.rs` | 标签栏渲染 |
| `amux-cli/src/tui/ui/sidebar.rs` | 侧边栏渲染 |
| `amux-cli/src/tui/ui/git_panel.rs` | Git 面板渲染 |
| `amux-cli/src/tui/ui/terminal.rs` | 终端渲染 |
| `amux-cli/src/tui/ui/diff.rs` | Diff 渲染 |
| `amux-cli/src/tui/ui/overlays.rs` | 弹窗渲染 |
| `amux-cli/src/tui/ui/helpers.rs` | 辅助函数 |
| `amux-cli/src/tui/input/mod.rs` | Input 主模块 |
| `amux-cli/src/tui/input/navigation.rs` | 导航输入 |
| `amux-cli/src/tui/input/terminal.rs` | 终端输入 |
| `amux-cli/src/tui/input/prefix.rs` | 前缀键处理 |
| `amux-cli/src/tui/input/dialogs.rs` | 对话框输入 |
| `amux-cli/src/tui/input/diff.rs` | Diff 输入 |
| `amux-cli/src/tui/input/git_status.rs` | Git 状态输入 |
| `amux-cli/src/tui/input/todo.rs` | TODO 输入 |
| `amux-cli/src/tui/input/mouse.rs` | 鼠标输入 |
| `amux-cli/src/tui/input/text_input.rs` | 文本输入处理器 |
| `amux-daemon/src/handlers/mod.rs` | Handlers 主模块 |
| `amux-daemon/src/handlers/repo.rs` | 仓库处理器 |
| `amux-daemon/src/handlers/worktree.rs` | Worktree 处理器 |
| `amux-daemon/src/handlers/session.rs` | 会话处理器 |
| `amux-daemon/src/handlers/events.rs` | 事件处理器 |
| `amux-daemon/src/handlers/diff.rs` | Diff 处理器 |
| `amux-daemon/src/handlers/comments.rs` | 评论处理器 |
| `amux-daemon/src/handlers/git_status.rs` | Git 状态处理器 |
| `amux-daemon/src/handlers/todo.rs` | TODO 处理器 |
