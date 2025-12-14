# TODO List Feature Design

## Overview

Repository-level TODO list feature for amux, accessible via floating popup modal with nested sub-item support.

## User Requirements

- **Scope**: Repository-level (shared across all branches/worktrees)
- **UI**: Floating popup triggered by hotkey
- **Data Model**: Title + Description + Sub-items support

## Data Model

### TodoItem Structure

```rust
pub struct TodoItem {
    pub id: String,
    pub repo_id: String,
    pub title: String,
    pub description: Option<String>,
    pub completed: bool,
    pub parent_id: Option<String>,  // For nested sub-items
    pub order: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

### Storage

- **Location**: `~/.amux/todos/{repo_id}/todos.json`
- **Format**: JSON with pretty printing
- **Persistence**: Automatic save on every change

### Example JSON

```json
{
  "items": [
    {
      "id": "uuid-1",
      "repo_id": "repo-hash",
      "title": "Implement feature X",
      "description": "Need to add validation and tests",
      "completed": false,
      "parent_id": null,
      "order": 0,
      "created_at": "2024-01-15T10:30:00Z",
      "updated_at": "2024-01-15T10:30:00Z"
    },
    {
      "id": "uuid-2",
      "repo_id": "repo-hash",
      "title": "Add validation",
      "description": null,
      "completed": true,
      "parent_id": "uuid-1",
      "order": 0,
      "created_at": "2024-01-15T10:31:00Z",
      "updated_at": "2024-01-15T11:00:00Z"
    }
  ]
}
```

## Hotkeys

| Context | Key | Action |
|---------|-----|--------|
| Global | `Ctrl+t` | Open TODO popup |
| Prefix | `Ctrl+s t` | Open TODO popup |
| Popup | `Esc` / `q` | Close popup |
| Popup | `j` / `Down` | Move cursor down |
| Popup | `k` / `Up` | Move cursor up |
| Popup | `l` / `Right` | Expand item (show sub-items/description) |
| Popup | `h` / `Left` | Collapse item |
| Popup | `Space` / `Enter` | Toggle completion |
| Popup | `a` | Add new TODO |
| Popup | `A` (Shift+a) | Add sub-item under current |
| Popup | `e` | Edit title |
| Popup | `E` (Shift+e) | Edit description |
| Popup | `d` / `Delete` | Delete (with confirmation) |
| Popup | `K` (Shift+k) | Move item up |
| Popup | `J` (Shift+j) | Move item down |
| Popup | `c` | Toggle completed visibility |

## UI Design

### Popup Layout

```
+------------------- TODOs (3/5) -------------------+
| > [ ] Implement feature X                         |
|     v [x] Add validation                          |
|         Design the UI                             |
|     > [ ] Write tests                             |
|   [ ] Fix bug in auth                             |
|   [x] Update documentation                        |
+---------------------------------------------------+
| a:add A:sub e:edit Space:toggle d:del Esc:close   |
+---------------------------------------------------+
```

### Visual Elements

- `>` Cursor indicator (highlighted in yellow)
- `[ ]` Uncompleted checkbox
- `[x]` Completed checkbox (green, strikethrough text)
- `v` Expanded parent (shows children)
- `>` Collapsed parent (has hidden children)
- Indentation: 2 spaces per nesting level
- Description shown in italic gray below expanded items

### Popup Dimensions

- Width: 70% of screen (min 40, max 100 chars)
- Height: 70% of screen (min 15, max 40 lines)
- Centered on screen

## Architecture

### Proto Definitions (daemon.proto)

```protobuf
// Messages
message TodoItem { ... }
message CreateTodoRequest { repo_id, title, description?, parent_id? }
message UpdateTodoRequest { todo_id, title?, description?, completed?, order? }
message DeleteTodoRequest { todo_id }
message ListTodosRequest { repo_id, include_completed? }
message ListTodosResponse { repeated TodoItem items }
message ToggleTodoRequest { todo_id }
message ReorderTodoRequest { todo_id, new_order, new_parent_id? }

// RPCs
rpc CreateTodo(CreateTodoRequest) returns (TodoItem);
rpc UpdateTodo(UpdateTodoRequest) returns (TodoItem);
rpc DeleteTodo(DeleteTodoRequest) returns (Empty);
rpc ListTodos(ListTodosRequest) returns (ListTodosResponse);
rpc ToggleTodo(ToggleTodoRequest) returns (TodoItem);
rpc ReorderTodo(ReorderTodoRequest) returns (TodoItem);
```

### Daemon Module (todo.rs)

```rust
pub struct TodoOps;

impl TodoOps {
    pub fn load_todos(repo_id: &str) -> Result<RepoTodos>;
    pub fn save_todos(repo_id: &str, todos: &RepoTodos) -> Result<()>;
    pub fn create_todo(...) -> Result<TodoItem>;
    pub fn update_todo(...) -> Result<TodoItem>;
    pub fn delete_todo(repo_id: &str, todo_id: &str) -> Result<()>;
    pub fn toggle_todo(repo_id: &str, todo_id: &str) -> Result<TodoItem>;
    pub fn reorder_todo(...) -> Result<TodoItem>;
    pub fn list_todos(repo_id: &str, include_completed: bool) -> Result<Vec<TodoItem>>;
    pub fn find_todo(repo_id: &str, todo_id: &str) -> Result<Option<TodoItem>>;
}
```

### Client Methods (client.rs)

```rust
impl Client {
    pub async fn create_todo(...) -> Result<TodoItem>;
    pub async fn update_todo(...) -> Result<TodoItem>;
    pub async fn delete_todo(todo_id: &str) -> Result<()>;
    pub async fn list_todos(repo_id: &str, include_completed: bool) -> Result<Vec<TodoItem>>;
    pub async fn toggle_todo(todo_id: &str) -> Result<TodoItem>;
    pub async fn reorder_todo(...) -> Result<TodoItem>;
}
```

### TUI State (app.rs)

```rust
// New InputMode variants
pub enum InputMode {
    // ... existing ...
    TodoPopup,
    AddTodo { parent_id: Option<String> },
    EditTodo { todo_id: String },
    EditTodoDescription { todo_id: String },
    ConfirmDeleteTodo { todo_id: String, title: String },
}

// New AsyncAction variants
pub enum AsyncAction {
    // ... existing ...
    LoadTodos,
    CreateTodo { title, description, parent_id },
    ToggleTodo { todo_id },
    DeleteTodo { todo_id },
    UpdateTodo { todo_id, title, description },
}

// App state fields
pub struct App {
    // ... existing ...
    pub todo_popup_visible: bool,
    pub todo_items: Vec<TodoItem>,
    pub todo_cursor: usize,
    pub expanded_todos: HashSet<String>,
    pub todo_scroll_offset: usize,
    pub todo_show_completed: bool,
}
```

## Implementation Files

| File | Changes |
|------|---------|
| `amux-proto/proto/daemon.proto` | TODO messages + RPCs |
| `amux-daemon/src/todo.rs` | **NEW** - Persistence module |
| `amux-daemon/src/main.rs` | Add `mod todo;` |
| `amux-daemon/src/server.rs` | TODO RPC handlers |
| `amux-cli/src/client.rs` | TODO client methods |
| `amux-cli/src/tui/app.rs` | State fields + InputMode + AsyncAction |
| `amux-cli/src/tui/input.rs` | TODO popup input handlers |
| `amux-cli/src/tui/ui.rs` | Popup rendering functions |

## Implementation Phases

### Phase 1: Proto & Daemon
1. Add proto messages to `daemon.proto`
2. Create `todo.rs` with persistence logic
3. Implement RPC handlers in `server.rs`

### Phase 2: Client
1. Add TODO client methods to `client.rs`

### Phase 3: TUI State
1. Add state fields to `App` struct
2. Add `InputMode` variants
3. Add `AsyncAction` variants

### Phase 4: TUI Input
1. Add `handle_todo_popup_sync()` handler
2. Add `handle_add_todo_input_sync()` handler
3. Add `Ctrl+t` detection in main input handler
4. Add `t` command in prefix mode

### Phase 5: TUI Rendering
1. `draw_todo_popup()` - Main popup
2. `draw_add_todo_overlay()` - Add input box
3. `draw_edit_todo_overlay()` - Edit input box
4. `draw_confirm_delete_todo_overlay()` - Delete confirmation

### Phase 6: Async Action Handling
1. Add match arms in `execute_async_action()`
2. Implement TODO helper methods

## Design Decisions

### Repository-Level vs Branch-Level
**Choice**: Repository-level (shared across branches)
**Rationale**: TODOs often span multiple branches; avoids duplication when switching

### Nested Items Implementation
**Choice**: Flat list with `parent_id` references
**Rationale**: Simpler to serialize/deserialize; flexible for reordering

### Popup vs Panel
**Choice**: Floating popup modal
**Rationale**: Doesn't consume screen real estate; accessible from any focus state

### Ordering Strategy
**Choice**: Explicit `order` field with auto-reordering
**Rationale**: Preserves user-defined order; allows drag-and-drop style reordering

## Future Enhancements

- [ ] Due dates
- [ ] Priority levels (Low/Medium/High)
- [ ] Tags/labels
- [ ] Export to markdown
- [ ] Sync with external TODO systems
- [ ] Keyboard shortcuts customization
