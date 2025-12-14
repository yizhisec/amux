# amux

**Agent MUX** - 像 tmux 管理终端一样管理 AI Agent 会话

amux 是一个终端复用器，专为管理多个 AI Agent（如 Claude）会话而设计。它提供了类似 tmux 的体验，让你可以同时管理多个仓库的多个 Agent 会话。

## 功能特性

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
| `Ctrl-s n` | 新建会话 |
| `Ctrl-s g` | 打开 Git 状态面板 |
| `Ctrl-s v` | 打开 Diff 视图 |
| `Ctrl-s o` | 打开 Todo 列表 |
| `Ctrl-s f` | 全屏切换 |
| `Ctrl-s q` | 退出 |

### 侧边栏

| 快捷键 | 功能 |
|--------|------|
| `j/k` | 上下移动 |
| `Enter` | 选择/进入 |
| `n` | 新建会话 |
| `a` | 添加 Worktree |
| `d` | 删除当前项 |
| `R` | 重命名会话 |
| `1-9` | 快速切换仓库 |

### Git 状态面板

| 快捷键 | 功能 |
|--------|------|
| `s` | 暂存文件 |
| `u` | 取消暂存 |
| `S` | 暂存全部 |
| `U` | 取消暂存全部 |
| `p` | 拉取 (pull) |
| `P` | 推送 (push) |

### 终端

| 快捷键 | 功能 |
|--------|------|
| `Ctrl-s [` | 进入普通模式（可滚动） |
| `i` / `Enter` | 进入插入模式 |
| `j/k` | 滚动 |
| `u/d` | 半页滚动 |
| `g/G` | 跳到顶部/底部 |

## 配置

配置文件位于 `~/.amux/config.toml`：

```toml
[prefix]
key = "C-s"

[options]
mouse_enabled = false
fullscreen_on_connect = false
show_completed_todos = false

[ui]
sidebar_width = 30
terminal_scrollback = 10000
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
