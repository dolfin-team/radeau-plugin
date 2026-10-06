# radeau-plugin

Protocol shared by [`radeau`](https://github.com/dolfin-team/raft), the CLI for the
[Dolfin](https://www.dolfin.fr) ontology language, and its external `radeau-*` plugins.

A plugin is any executable named `radeau-<name>` on `PATH`. `radeau <name> ...` runs it
as `radeau-<name> <name> ...` (the same trick `cargo` uses for `cargo-fmt`). This crate
handles the plumbing so a plugin's `main` stays a single call.

## What it provides

- **Shell completion**: radeau calls `radeau-<name> <name> --radeau-completion-spec`.
  The plugin prints its clap command tree as JSON (`to_spec`); radeau rebuilds it
  (`from_spec`) so plugin flags complete like built-in ones. A plugin that ignores the
  flag exits non-zero and only its name completes.
- **Interactive prompts** (`interactive`): run in a terminal with missing or no
  arguments, the user is prompted for exactly what clap reports missing, then offered
  the remaining options. Prompts come from the clap definitions. The resulting command
  line is printed so the user learns the direct form.
- **`run`**: does all of the above, parses argv, runs your body and prints errors with
  their cause chain.

## Usage

```toml
[dependencies]
radeau-plugin = "0.1"
clap = { version = "4", features = ["derive"] }
```

```rust
use clap::Parser;
use std::process::ExitCode;

/// Render the ontology as a diagram.
#[derive(Parser)]
#[command(name = "radeau-viz")]
struct Cli {
    /// Input .dlf file.
    path: std::path::PathBuf,
    /// Output format.
    #[arg(long, value_parser = ["svg", "png"])]
    format: Option<String>,
}

fn main() -> ExitCode {
    radeau_plugin::run::<Cli, std::io::Error>("viz", |cli| {
        // ...
        Ok(())
    })
}
```

Works both as `radeau viz ...` and directly as `radeau-viz ...`.

For finer control, use the pieces separately: strip the echo, then call
`answer_spec_request::<Cli>(&args)` and `interactive::<Cli>(args)` before parsing.

Init importers follow the same pattern: `radeau-init-<format>`, invoked as
`radeau init <format>` with echo `<format>`.

## Development

```sh
cargo test
cargo fmt && cargo clippy
```

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

MIT, see [LICENSE](LICENSE).
