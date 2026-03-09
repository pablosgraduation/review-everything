//! `re` — interactive TUI diff viewer powered by difftastic.

mod app;
mod difft;
mod git;
mod input;
mod integrity;
mod nav;
mod processor;
mod types;
mod ui;

use clap::Parser;

#[derive(Parser)]
#[command(name = "re", about = "review-everything: TUI diff viewer built on difftastic")]
struct Cli {
    /// Git revision or range (e.g., abc123, main..feature)
    #[arg(value_name = "REVSPEC")]
    revspec: Option<String>,

    /// Show staged changes (HEAD vs INDEX)
    #[arg(long)]
    staged: bool,

    /// Show unstaged changes (INDEX vs WORKING TREE)
    #[arg(long)]
    unstaged: bool,

    /// Hide file tree sidebar
    #[arg(long = "no-tree")]
    no_tree: bool,

    /// Tree sidebar width
    #[arg(long = "tree-width", default_value = "35")]
    tree_width: u16,

    /// Auto-hide tree when unfocused
    #[arg(long = "auto-hide-tree")]
    auto_hide_tree: bool,

    /// Highlight mode
    #[arg(long, default_value = "difftastic", value_parser = ["difftastic", "none"])]
    highlight: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    // Verify we're in a git repo
    git::git_root().map_err(|_| "Not a git repository (or any parent up to mount point)")?;

    // Verify difft is installed
    if std::process::Command::new("difft")
        .arg("--version")
        .output()
        .is_err()
    {
        return Err("difft not found. Install difftastic: https://difftastic.wilfred.me.uk/".into());
    }

    let mut app = app::App::new();
    app.show_tree = !cli.no_tree;
    app.tree_width = cli.tree_width;
    app.auto_hide_tree = cli.auto_hide_tree;

    // Determine mode from CLI args
    let direct_mode = if cli.staged {
        Some(app::DiffMode::Staged)
    } else if cli.unstaged {
        Some(app::DiffMode::Unstaged)
    } else {
        cli.revspec.as_ref().map(|revspec| app::DiffMode::Range(revspec.clone()))
    };

    // Initialize terminal
    let mut terminal = ratatui::init();

    let result = if let Some(mode) = direct_mode {
        // Direct diff mode - skip log view
        app.launched_with_args = true;
        let context = match &mode {
            app::DiffMode::Staged => Some("staged changes".to_string()),
            app::DiffMode::Unstaged => Some("unstaged changes".to_string()),
            app::DiffMode::Range(r) => Some(r.clone()),
            app::DiffMode::WorkingTree(c) => Some(format!("{c} vs working tree")),
            app::DiffMode::StagedVsCommit(c) => Some(format!("{c} vs staged")),
        };

        app.start_diff_loading(mode, context);
        app.run(&mut terminal)
    } else {
        // Log view mode
        match app.load_log() {
            Ok(()) => app.run(&mut terminal),
            Err(e) => {
                app.view = app::View::Error(e);
                app.run(&mut terminal)
            }
        }
    };

    ratatui::restore();

    if let Err(e) = result {
        eprintln!("Error: {e}");
        std::process::exit(1);
    }

    Ok(())
}
