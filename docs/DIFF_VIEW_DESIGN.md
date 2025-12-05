# CCM Diff View Design

Add a Diff View to ccm TUI, allowing users to view git diff alongside the terminal view, with GitHub-style code review annotations.

## UI Layout

```
┌─────────────────────────────────────────────────────────────┐
│ [1:repo] [2:repo2]                                         │
├────────────────┬────────────────────────────────────────────┤
│ Worktrees      │  [d] Diff View  /  [t] Terminal View      │
│ ● main         │  ┌──────────────┬─────────────────────────┐│
│   feature      │  │ Diff Files   │ Diff Content            ││
│                │  │ M src/main.rs│ @@ -1,3 +1,5 @@        ││
│ Sessions       │  │ A src/new.rs │ - old line             ││
│ ▶ session1     │  │ D src/old.rs │ + new line             ││
│                │  └──────────────┴─────────────────────────┘│
└────────────────┴────────────────────────────────────────────┘
```

## Key Decisions

1. **Diff Source**: Daemon provides diff data via gRPC (TUI may run in different directory)
2. **Default Diff**: `git diff` (working tree vs HEAD)
3. **Proto Changes**: Add `GetDiffFiles` and `GetFileDiff` RPCs
4. **Syntax Highlighting**: Use [syntect](https://crates.io/crates/syntect) for syntax highlighting

---

## Phase 1: Basic Diff View (MVP)

### Proto Definitions

**File:** `ccm-proto/proto/daemon.proto`

```protobuf
service CcmDaemon {
    // ... existing methods ...

    // Diff operations
    rpc GetDiffFiles(GetDiffFilesRequest) returns (GetDiffFilesResponse);
    rpc GetFileDiff(GetFileDiffRequest) returns (GetFileDiffResponse);
}

message GetDiffFilesRequest {
    string repo_id = 1;
    string branch = 2;  // worktree branch
}

message GetDiffFilesResponse {
    repeated DiffFileInfo files = 1;
}

message DiffFileInfo {
    string path = 1;
    FileStatus status = 2;
    int32 additions = 3;
    int32 deletions = 4;
}

enum FileStatus {
    FILE_STATUS_UNSPECIFIED = 0;
    FILE_STATUS_MODIFIED = 1;
    FILE_STATUS_ADDED = 2;
    FILE_STATUS_DELETED = 3;
    FILE_STATUS_RENAMED = 4;
    FILE_STATUS_UNTRACKED = 5;
}

message GetFileDiffRequest {
    string repo_id = 1;
    string branch = 2;
    string file_path = 3;
}

message GetFileDiffResponse {
    string file_path = 1;
    repeated DiffLine lines = 2;
}

message DiffLine {
    LineType line_type = 1;
    string content = 2;
}

enum LineType {
    LINE_TYPE_UNSPECIFIED = 0;
    LINE_TYPE_HEADER = 1;     // @@ ... @@
    LINE_TYPE_CONTEXT = 2;    // unchanged
    LINE_TYPE_ADDITION = 3;   // +
    LINE_TYPE_DELETION = 4;   // -
}
```

### Daemon Diff Module

**New File:** `ccm-daemon/src/diff.rs`

```rust
use crate::error::GitError;
use git2::{Repository, DiffOptions, Delta};
use std::path::Path;

pub struct DiffOps;

impl DiffOps {
    /// Get list of changed files in worktree (vs HEAD)
    pub fn get_diff_files(worktree_path: &Path) -> Result<Vec<DiffFileInfo>, GitError>;

    /// Get diff content for a specific file
    pub fn get_file_diff(worktree_path: &Path, file_path: &str) -> Result<Vec<DiffLine>, GitError>;
}
```

### TUI State Changes

**File:** `ccm-cli/src/tui/app.rs`

```rust
pub enum Focus {
    Branches,
    Sessions,
    Terminal,
    DiffFiles,    // NEW
    DiffContent,  // NEW
}

pub enum RightPanelView {
    Terminal,
    Diff,
}

// Add to App struct:
pub right_panel_view: RightPanelView,
pub diff_files: Vec<DiffFileInfo>,
pub diff_file_idx: usize,
pub diff_lines: Vec<DiffLine>,
pub diff_scroll_offset: usize,
pub diff_fullscreen: bool,
```

### Keybindings

Global key (when not in Insert mode):
- `d` - Switch to Diff view, refresh diff files

DiffFiles mode:
- `j/k` - Navigate file list
- `Enter` - View selected file diff
- `r` - Refresh diff
- `f` - Toggle fullscreen
- `Esc` - Back to Terminal view

DiffContent mode:
- `j/k` - Scroll line by line
- `Ctrl+d/u` - Page down/up
- `g/G` - Top/Bottom
- `f` - Toggle fullscreen
- `Esc` - Back to file list

### Syntax Highlighting

**Dependencies:**
```toml
# ccm-cli/Cargo.toml
syntect = { version = "5", default-features = false, features = ["default-syntaxes", "default-themes", "regex-onig"] }
```

**New module:** `ccm-cli/src/tui/highlight.rs`

```rust
use syntect::highlighting::{ThemeSet, Style};
use syntect::parsing::SyntaxSet;
use syntect::easy::HighlightLines;

pub struct Highlighter {
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
}

impl Highlighter {
    pub fn new() -> Self;
    pub fn highlight_line(&self, line: &str, extension: &str) -> Vec<(Style, &str)>;
    pub fn to_ratatui_style(style: &Style) -> ratatui::style::Style;
}
```

**Diff line colors (on top of syntax highlighting):**
- Header (`@@`): Cyan background
- Addition (`+`): Green tint / left border
- Deletion (`-`): Red tint / left border
- Context: Default background

### Files to Modify

| File | Change |
|------|--------|
| `ccm-proto/proto/daemon.proto` | Add diff RPCs and messages |
| `ccm-daemon/src/main.rs` | Export `diff` module |
| `ccm-daemon/src/diff.rs` | **NEW** - git2 diff operations |
| `ccm-daemon/src/server.rs` | Implement diff handlers |
| `ccm-cli/src/client.rs` | Add diff client methods |
| `ccm-cli/src/tui/app.rs` | Add state, enums, methods |
| `ccm-cli/src/tui/input.rs` | Add diff input handlers |
| `ccm-cli/src/tui/ui.rs` | Add diff rendering |
| `ccm-cli/src/tui/highlight.rs` | **NEW** - Syntect wrapper |
| `ccm-cli/Cargo.toml` | Add syntect dependency |

---

## Phase 2: Enhanced Diff Display

- Word-level diff highlighting (similar to delta)
- Side-by-side view (optional)
- Custom theme support

---

## Phase 3: Code Review Annotation (GitHub PR Review Style)

### UI Effect

```
┌─────────────────────────────────────────────────────────────┐
│  src/main.rs                                    [3 comments]│
├─────────────────────────────────────────────────────────────┤
│  42 │ fn complex_function() {                               │
│  43 │+    let x = calculate();           [💬] ← comment     │
│     │  ┌─────────────────────────────────────────────────┐  │
│     │  │ 💬 Why use calculate()? Consider lazy eval      │  │
│     │  └─────────────────────────────────────────────────┘  │
│  44 │     process(x);                                       │
│  45 │-    old_code();                                       │
│  46 │+    new_code();                    [💬]               │
│     │  ┌─────────────────────────────────────────────────┐  │
│     │  │ 💬 Nice refactoring, but needs tests            │  │
│     │  └─────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

### Data Structures

```rust
pub struct LineComment {
    pub id: String,
    pub file_path: String,
    pub line_number: u32,        // Line the comment is attached to
    pub line_type: LineType,     // Addition/Deletion/Context
    pub comment: String,
    pub created_at: DateTime,
}

pub struct ReviewSession {
    pub id: String,
    pub repo_id: String,
    pub branch: String,
    pub comments: Vec<LineComment>,
    pub status: ReviewStatus,    // Draft/Submitted
}
```

### Persistence

```
~/.ccm/reviews/{repo_id}/{branch}/
└── review.json   # ReviewSession
```

### TUI Interaction

In Diff Content mode:
- `c` - Add comment on current line (opens input popup)
- `C` - View/edit comment on current line
- `x` - Delete comment on current line
- `n/N` - Jump to next/previous comment
- `S` - **Submit Review**: Batch send all comments to Claude

### Send to Claude (Batch)

When pressing `S`, generate formatted content and inject into PTY:

```
Please help me review the following code changes:

## File: src/main.rs

### Line 43 (+)
```rust
let x = calculate();
```
💬 Comment: Why use calculate()? Consider lazy eval

### Line 46 (+)
```rust
new_code();
```
💬 Comment: Nice refactoring, but needs tests

---
Please provide your suggestions for the above comments.
```

Claude's response displays in the **Terminal panel**.

### Proto Extensions

```protobuf
// Review management
rpc CreateLineComment(CreateLineCommentRequest) returns (LineCommentInfo);
rpc UpdateLineComment(UpdateLineCommentRequest) returns (LineCommentInfo);
rpc DeleteLineComment(DeleteLineCommentRequest) returns (Empty);
rpc ListLineComments(ListLineCommentsRequest) returns (ListLineCommentsResponse);
rpc SubmitReviewToClaude(SubmitReviewRequest) returns (Empty);  // Batch send
```

---

## Future Enhancements

- Branch-to-branch diff comparison
- Interactive staging/unstaging
- Annotation search and filtering
- Export annotations to markdown
