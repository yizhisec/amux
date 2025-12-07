# Git Status 面板设计文档

## 目标
在侧边栏 Worktrees 面板下方添加 Git Status 面板，显示文件状态，支持 stage/unstage 操作，选中文件时联动 Diff 面板。

## 布局设计

```
┌─────────────────────┐
│    Worktrees (60%)  │
│  ▼ main (2)         │
│    ▶ session-1      │
│  ▼ feature-x        │
├─────────────────────┤
│   Git Status (40%)  │
│ ◆ Staged (2)        │
│   M src/app.rs      │
│   A src/new.rs      │
│ ◇ Unstaged (1)      │
│   M src/ui.rs       │
│ ? Untracked (1)     │
│   README.md         │
└─────────────────────┘
```

## 键盘快捷键

| 快捷键 | 上下文 | 功能 |
|--------|--------|------|
| `g` | 侧边栏 | 切换到 Git Status 面板 |
| `j/k` | Git Status | 上下导航 |
| `o` / `Enter` | Git Status (分区) | 展开/折叠分区 |
| `Enter` | Git Status (文件) | 在 Diff 面板显示该文件差异 |
| `s` | Git Status (文件) | Stage 文件 |
| `u` | Git Status (文件) | Unstage 文件 |
| `S` | Git Status | Stage 所有文件 |
| `U` | Git Status | Unstage 所有文件 |
| `r` | Git Status | 刷新状态 |
| `Tab` | Git Status | 切换到 Diff 面板 |
| `Esc` | Git Status | 返回 Worktrees 面板 |

## 数据结构设计

### GitSection 枚举
```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum GitSection {
    Staged,
    Unstaged,
    Untracked,
}
```

### GitStatusFile 结构
```rust
#[derive(Debug, Clone)]
pub struct GitStatusFile {
    pub path: String,
    pub status: FileStatus,  // 复用现有 FileStatus
    pub staged: bool,
    pub section: GitSection,
}
```

### GitPanelItem 枚举
```rust
#[derive(Debug, Clone, PartialEq)]
pub enum GitPanelItem {
    Section(GitSection),  // 分区标题
    File(usize),          // 文件索引
    None,
}
```

### App 新增状态字段
```rust
pub struct App {
    // ... 现有字段 ...

    // Git Status 面板状态
    pub git_panel_enabled: bool,
    pub git_panel_collapsed: bool,
    pub git_status_files: Vec<GitStatusFile>,
    pub git_status_cursor: usize,
    pub expanded_git_sections: HashSet<GitSection>,
    pub git_panel_scroll_offset: usize,
}
```

## Proto 扩展

```protobuf
// Git Status 操作
rpc GetGitStatus(GetGitStatusRequest) returns (GetGitStatusResponse);
rpc StageFile(StageFileRequest) returns (Empty);
rpc UnstageFile(UnstageFileRequest) returns (Empty);
rpc StageAll(StageAllRequest) returns (Empty);
rpc UnstageAll(UnstageAllRequest) returns (Empty);

message GetGitStatusRequest {
    string repo_id = 1;
    string branch = 2;
}

message GetGitStatusResponse {
    repeated GitStatusFile staged = 1;
    repeated GitStatusFile unstaged = 2;
    repeated GitStatusFile untracked = 3;
}

message GitStatusFile {
    string path = 1;
    FileStatus status = 2;
}

message StageFileRequest {
    string repo_id = 1;
    string branch = 2;
    string file_path = 3;
}

message UnstageFileRequest {
    string repo_id = 1;
    string branch = 2;
    string file_path = 3;
}

message StageAllRequest {
    string repo_id = 1;
    string branch = 2;
}

message UnstageAllRequest {
    string repo_id = 1;
    string branch = 2;
}
```

## Git 操作实现

### get_status
```rust
pub fn get_status(repo: &Repository) -> Result<GitStatusResult, GitError> {
    let statuses = repo.statuses(Some(
        StatusOptions::new()
            .include_untracked(true)
            .recurse_untracked_dirs(true)
    ))?;

    // 分类：staged (INDEX_*), unstaged (WT_*), untracked (WT_NEW)
}
```

### stage_file
```rust
pub fn stage_file(repo: &Repository, path: &str) -> Result<(), GitError> {
    let mut index = repo.index()?;
    index.add_path(Path::new(path))?;
    index.write()?;
    Ok(())
}
```

### unstage_file
```rust
pub fn unstage_file(repo: &Repository, path: &str) -> Result<(), GitError> {
    let head = repo.head()?.peel_to_commit()?;
    repo.reset_default(Some(&head.into_object()), &[Path::new(path)])?;
    Ok(())
}
```

## Diff 面板联动

当 Git Status 面板选中文件时：
1. 自动切换 `right_panel_view` 为 `RightPanelView::Diff`
2. 定位到对应文件在 `diff_files` 中的位置
3. 自动展开该文件，触发 `LoadFileDiff` 加载差异内容

```rust
pub fn git_status_selection_changed(&mut self) -> Option<AsyncAction> {
    if let GitPanelItem::File(file_idx) = self.current_git_panel_item() {
        if let Some(file) = self.git_status_files.get(file_idx) {
            self.right_panel_view = RightPanelView::Diff;

            if let Some(idx) = self.diff_files.iter().position(|f| f.path == file.path) {
                self.diff_cursor = idx;
                if !self.diff_expanded.contains(&idx) {
                    self.diff_expanded.insert(idx);
                    return Some(AsyncAction::LoadFileDiff);
                }
            }
        }
    }
    None
}
```

## 实现步骤

### 阶段 1: Proto 定义
**文件**: `ccm-proto/proto/daemon.proto`
- [ ] 新增 RPC: `GetGitStatus`, `StageFile`, `UnstageFile`, `StageAll`, `UnstageAll`
- [ ] 新增消息类型

### 阶段 2: Git 操作实现
**文件**: `ccm-daemon/src/git.rs`
- [ ] `get_status(repo)` - 获取文件状态
- [ ] `stage_file(repo, path)` - 暂存文件
- [ ] `unstage_file(repo, path)` - 取消暂存
- [ ] `stage_all(repo)` - 暂存所有
- [ ] `unstage_all(repo)` - 取消暂存所有

### 阶段 3: Server RPC 实现
**文件**: `ccm-daemon/src/server.rs`
- [ ] 实现 5 个新 RPC handler

### 阶段 4: Client 方法
**文件**: `ccm-cli/src/client.rs`
- [ ] 新增 5 个客户端方法

### 阶段 5: App 状态管理
**文件**: `ccm-cli/src/tui/app.rs`
- [ ] 新增数据结构: `GitSection`, `GitStatusFile`, `GitPanelItem`
- [ ] 新增 `Focus::GitStatus`
- [ ] 新增状态字段
- [ ] 新增 AsyncAction 变体
- [ ] 实现导航和联动方法

### 阶段 6: UI 渲染
**文件**: `ccm-cli/src/tui/ui.rs`
- [ ] 修改 `draw_sidebar()` 为双面板布局
- [ ] 新增 `draw_git_status_panel()`

### 阶段 7: 输入处理
**文件**: `ccm-cli/src/tui/input.rs`
- [ ] 新增 `handle_git_status_input_sync()`
- [ ] 修改 prefix 模式支持 `g` 键

## 关键文件清单

| 文件 | 修改内容 |
|------|----------|
| `ccm-proto/proto/daemon.proto` | Proto 定义 |
| `ccm-daemon/src/git.rs` | Git 操作核心 |
| `ccm-daemon/src/server.rs` | RPC 实现 |
| `ccm-cli/src/client.rs` | 客户端方法 |
| `ccm-cli/src/tui/app.rs` | 状态管理 |
| `ccm-cli/src/tui/ui.rs` | UI 渲染 |
| `ccm-cli/src/tui/input.rs` | 输入处理 |
