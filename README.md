<div align="center">
<pre>
 ██████╗ ███████╗
 ██╔══██╗██╔════╝
 ██████╔╝█████╗
 ██╔══██╗██╔══╝
 ██║  ██║███████╗
 ╚═╝  ╚═╝╚══════╝
</pre>
<pre>
 ___ _____   _____ _____      __  _____   _____ _____   _______ _  _ ___ _  _  ___
| _ \ __\ \ / /_ _| __\ \    / / | __\ \ / / __| _ \ \ / /_   _| || |_ _| \| |/ __|
|   / _| \ V / | || _| \ \/\/ /  | _| \ V /| _||   /\ V /  | | | __ || || .` | (_ |
|_|_\___| \_/ |___|___| \_/\_/   |___| \_/ |___|_|_\ |_|   |_| |_||_|___|_|\_|\___|
</pre>

Interactive TUI diff viewer powered by [difftastic](https://difftastic.wilfred.me.uk/).

<!-- TODO: add screenshot or demo GIF -->
</div>

## Features

- **Commit log** — browse recent commits with search filtering
- **Side-by-side diff** — syntax-aware character-level highlights from difftastic
- **File tree** — collapsible directory sidebar with change stats
- **Hunk navigation** — jump between changes within and across files
- **Compare flow** — diff any two endpoints (commits, staged, unstaged, working tree)
- **Search** — filter commits and compare items by keyword
- **Scrollbar** — color-coded change markers for quick orientation

## Prerequisites

- [difftastic](https://difftastic.wilfred.me.uk/) (`difft`) must be installed and on your `PATH`
- A git repository

## Installation

```sh
# From source (cloned repo)
cargo install --path .

# From GitHub
cargo install --git https://github.com/pablosgraduation/review-everything

# From crates.io (once published)
cargo install review-everything
```

## Update

Re-run the same install command:

```sh
# From GitHub
cargo install --git https://github.com/pablosgraduation/review-everything

# From source
git pull && cargo install --path .
```

## Uninstall

```sh
cargo uninstall review-everything
```

## Usage

```sh
# Open commit log (default)
re

# Show staged changes
re --staged

# Show unstaged changes
re --unstaged

# View a single commit
re abc123

# Compare a range
re main..feature
re main...feature

# Hide file tree sidebar
re --no-tree

# Custom tree width (default: 35)
re --tree-width 50
```

## Key Bindings

| Key | Action |
|-----|--------|
| `j` / `Down` | Scroll down |
| `k` / `Up` | Scroll up |
| `Shift+Down/Up` | Scroll 5 lines |
| `Ctrl+d` / `Ctrl+u` | Half page down/up |
| `gg` / `G` | Top / bottom of file |
| `h` / `l` | Scroll left / right |
| `Ctrl+Shift+Down/Up` | Next / previous hunk |
| `]f` / `[f` | Next / previous file |
| `Tab` | Toggle tree / diff focus |
| `Enter` | Select / view diff |
| `c` | Compare two endpoints (from log) |
| `/` | Search (log / compare) |
| `?` | Toggle help |
| `q` / `Esc` | Quit / back |

## License

Licensed under either of

- [Apache License, Version 2.0](LICENSE-APACHE)
- [MIT License](LICENSE-MIT)

at your option.
