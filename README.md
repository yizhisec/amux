# amux

**Agent MUX** - 像 tmux 管理终端一样管理 AI Agent 会话

amux 是一个终端复用器，专为管理多个 AI Agent 会话而设计。它提供了类似 tmux 的体验，让你可以同时管理多个仓库的多个 Agent 会话。

## 功能特性

- **多 Provider 支持** - 支持 Claude Code 和 Codex
- **多仓库支持** - 同时管理多个 Git 仓库
- **会话管理** - 为每个分支创建独立的 Agent 会话
- **Git 集成** - 内置 Git 状态查看、暂存、提交、推送、拉取
- **Diff 视图** - 查看代码变更，支持语法高亮
- **Todo 管理** - 跟踪每个仓库的待办事项
- **Vim 风格快捷键** - 熟悉的键盘操作方式
- **自动 Worktree** - 自动创建 Git worktree，存储在 `~/.amux/repos/`

## 安装

### 一键安装（推荐）

```bash
curl -fsSL https://raw.githubusercontent.com/yizhisec/amux/main/install.sh | sh
```

### 从源码构建

```bash
git clone https://github.com/yizhisec/amux.git
cd amux
cargo build --release

# 安装到 ~/.local/bin
cargo xtask install
```

## 快速开始

```bash
# 启动 amux（会自动启动 daemon）
amux

# 或者手动启动 daemon
amux-daemon &
amux
```

## 键盘快捷键

### 前缀键：`Ctrl-s`

| 快捷键 | 功能 |
|--------|------|
| `Ctrl-s b` | 聚焦分支列表 |
| `Ctrl-s s` | 聚焦会话列表 |
| `Ctrl-s t` | 聚焦终端 |
| `Ctrl-s w` | 聚焦侧边栏 |
| `Ctrl-s n` | 新建会话 |
| `Ctrl-s N` | 选择 Provider 并新建会话 |
| `Ctrl-s a` | 添加 Worktree |
| `Ctrl-s d` | 删除当前项 |
| `Ctrl-s g` | 打开 Git 状态面板 |
| `Ctrl-s v` | 打开 Diff 视图 |
| `Ctrl-s o` | 打开 Todo 列表 |
| `Ctrl-s [` | 终端普通模式 |
| `Ctrl-s f/z` | 全屏切换 |
| `Ctrl-s r` | 刷新 |
| `Ctrl-s 1-9` | 快速切换仓库 |
| `Ctrl-s q` | 退出 |

### 侧边栏

| 快捷键 | 功能 |
|--------|------|
| `j/k` | 上下移动 |
| `Enter` | 选择/进入 |
| `o` | 展开/折叠 |
| `T` | 切换树视图 |
| `n` | 新建会话 |
| `N` | 选择 Provider 并新建 |
| `a` | 添加 Worktree |
| `d/x` | 删除当前项 |
| `R` | 重命名会话 |
| `t` | 切换 Diff 视图 |
| `g` | 打开 Git 状态 |
| `r` | 刷新 |
| `1-9` | 快速切换仓库 |
| `q` | 退出 |

### Git 状态面板

| 快捷键 | 功能 |
|--------|------|
| `j/k` | 上下移动 |
| `Enter/o` | 展开/折叠 |
| `s` | 暂存文件 |
| `u` | 取消暂存 |
| `S` | 暂存全部 |
| `U` | 取消暂存全部 |
| `p` | 拉取 (pull) |
| `P` | 推送 (push) |
| `r` | 刷新状态 |
| `Tab` | 切换到 Diff |
| `Esc/q` | 返回侧边栏 |

### 终端普通模式

| 快捷键 | 功能 |
|--------|------|
| `i/Enter` | 进入插入模式 |
| `j/k` | 上下滚动 |
| `u/d` | 半页滚动 |
| `g/G` | 跳到顶部/底部 |
| `f/z` | 全屏切换 |
| `Shift-Tab` | 退出终端 |
| `Esc` | 退出全屏 |

### Diff 视图

| 快捷键 | 功能 |
|--------|------|
| `j/k` | 上下移动 |
| `{/}` | 上/下一个文件 |
| `Enter/o` | 展开/折叠 |
| `c` | 添加评论 |
| `C` | 编辑评论 |
| `x` | 删除评论 |
| `n/N` | 下/上一条评论 |
| `S` | 提交 Review 给 Claude |
| `r` | 刷新 |
| `f/z` | 全屏切换 |
| `Esc/q/t` | 返回终端 |

### Todo 列表

| 快捷键 | 功能 |
|--------|------|
| `j/k` | 上下移动 |
| `g/G` | 跳到顶部/底部 |
| `Space` | 切换完成状态 |
| `n` | 新建 Todo |
| `N` | 新建子 Todo |
| `e` | 编辑标题 |
| `E` | 编辑描述 |
| `x` | 删除 |
| `J/K` | 上下移动项目 |
| `>/<` | 增加/减少缩进 |
| `H` | 显示/隐藏已完成 |
| `r` | 刷新 |
| `Esc/q` | 关闭 |

## 配置

配置文件位于 `~/.amux/config.toml`：

```toml
[prefix]
key = "C-s"

[options]
tree_view_enabled = true
git_panel_enabled = true
mouse_enabled = false
fullscreen_on_connect = false
show_completed_todos = false

[ui]
show_borders = true
sidebar_width = 30
terminal_scrollback = 10000

[providers]
default = "claude"  # 或 "codex"

[providers.claude]
enabled = true
command = "claude"
model = "sonnet"

[providers.codex]
enabled = true
command = "codex"
model = "o4-mini"
```

## 数据目录

```
~/.amux/
├── config.toml      # 配置文件
├── sessions/        # 会话数据
├── repos/           # Git worktrees
├── todos/           # Todo 数据
└── logs/            # 日志文件
```

## 许可证

AGPL-3.0-or-later

## 贡献

欢迎提交 Issue 和 Pull Request！
