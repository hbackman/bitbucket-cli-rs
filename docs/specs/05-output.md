# 05 — Output Formatting

**Status:** Draft (Rust)
**Depends on:** [`01-architecture.md`](01-architecture.md)
**Slice goal:** Every command can use a shared output helper to render its data as a colored table (TTY), TSV (pipe), JSON (`--json`), or jq-filtered JSON (`--jq`). Exit codes are standardized. Color rules respect `NO_COLOR` and friends.

## Output modes

For any list/view command, output is one of:

| Mode | Trigger | Shape |
| --- | --- | --- |
| Table | TTY stdout, no `--json` | Colored aligned columns |
| TSV | Non-TTY stdout, no `--json` | Tab-separated, no colors, no header |
| JSON (raw) | `--json` only | Pretty-printed JSON of requested fields |
| JSON (filtered) | `--json` + `--jq EXPR` | jq applied to the JSON form |
| Template | `--template EXPR` | (Post-MVP, via Tera) |

## `--json` flag pattern

Following `gh`:

```
bb pr list                              # table or TSV
bb pr list --json number,title,author   # JSON
bb pr list --json number,title,author --jq '.[] | .title'
bb pr list --json                       # error: lists available fields
```

`--json` with no value prints the available fields and exits 1. Discoverability feature, not an error.

Implementation: each command has a `JSON_FIELDS: &[&str]` constant declaring the available fields. A helper attaches `--json`, `--jq`, and (later) `--template` to the command and validates the requested fields.

```rust
// src/cli/json_flags.rs
#[derive(clap::Args, Debug, Default)]
pub struct JsonFlags {
    /// Comma-separated list of fields to output as JSON.
    /// Use `--json` with no value to list available fields.
    #[arg(long, value_name = "FIELDS", value_delimiter = ',', num_args = 0..)]
    pub json: Option<Vec<String>>,

    /// Filter JSON output with a jq expression.
    #[arg(long, value_name = "EXPR")]
    pub jq: Option<String>,

    /// Format JSON output with a Tera template (post-MVP).
    #[arg(long, value_name = "EXPR", hide = true)]
    pub template: Option<String>,
}

impl JsonFlags {
    pub fn validate(&self, available: &[&str]) -> Result<JsonMode, CliError> { /* ... */ }
}

pub enum JsonMode {
    Off,
    Fields(Vec<String>),
    FilterFields { fields: Vec<String>, jq: String },
}
```

`--json` filtering uses `jaq` (pure-Rust jq). We do **not** shell out to a system `jq`.

```rust
// src/cli/jq.rs
pub fn filter(input: &serde_json::Value, expr: &str) -> Result<Vec<serde_json::Value>, JqError> {
    let (filter, errs) = jaq_parse::parse(expr, jaq_parse::main());
    if !errs.is_empty() { return Err(JqError::Parse(errs)); }
    /* compile via jaq_interpret, run, collect results */
}
```

## Table renderer (`src/iostreams/table.rs`)

Lives next to `iostreams.rs` to access TTY detection and color directly.

```rust
pub struct TablePrinter<'a> {
    io: &'a mut IoStreams,
    columns: Vec<Column>,
    rows: Vec<Vec<String>>,
}

pub struct Column {
    pub header: String,
    pub wrap: bool,                      // truncate-with-ellipsis at terminal width
    pub colorize: Option<fn(&str) -> owo_colors::Styled<&str>>,
}

impl<'a> TablePrinter<'a> {
    pub fn new(io: &'a mut IoStreams, columns: Vec<Column>) -> Self;
    pub fn add_row(&mut self, cells: impl IntoIterator<Item = impl Into<String>>);
    pub fn render(self) -> std::io::Result<()>;
}
```

Behavior:
- **TTY**: headers in bold, aligned columns padded with spaces. Long cells truncated with ellipsis based on `terminal_size::terminal_size()`.
- **Non-TTY**: tab-separated, no header. Column padding stripped, color stripped.
- Headers are uppercase by convention (`NUMBER`, `TITLE`, `BRANCH`, `STATE`).

Multi-byte safe via `unicode-width::UnicodeWidthStr` for column-width computation.

## Color

Use `owo-colors` (style traits) layered with `anstream` (ANSI-stripping writer). Helpers in `src/iostreams.rs`:

```rust
impl IoStreams {
    pub fn color_enabled(&self) -> bool;            // already on the struct (spec 01)
    pub fn cs(&self) -> ColorScheme;                // borrowed view for command code
}

pub struct ColorScheme { enabled: bool }

impl ColorScheme {
    pub fn red<S: AsRef<str>>(&self, s: S) -> String;
    pub fn green<S: AsRef<str>>(&self, s: S) -> String;
    pub fn yellow<S: AsRef<str>>(&self, s: S) -> String;
    pub fn cyan<S: AsRef<str>>(&self, s: S) -> String;
    pub fn gray<S: AsRef<str>>(&self, s: S) -> String;
    pub fn bold<S: AsRef<str>>(&self, s: S) -> String;
    pub fn magenta<S: AsRef<str>>(&self, s: S) -> String;
    pub fn blue<S: AsRef<str>>(&self, s: S) -> String;
}
```

When `enabled == false`, every method returns the input unchanged. When enabled, it wraps with `owo_colors::OwoColorize` and renders via `to_string()`.

`color_enabled` precedence (already in spec 01's `detect_color`):
1. `CLICOLOR_FORCE=1` → on
2. `NO_COLOR` set (any value) → off
3. `CLICOLOR=0` → off
4. stdout is a TTY → on
5. otherwise → off

State-specific colors used across commands:

| State | Color |
| --- | --- |
| Open PR / running pipeline | Green |
| Merged PR | Magenta |
| Declined / failed | Red |
| Draft | Gray |
| Approved | Green |
| Requested changes | Red |

## Symbols

```
✓ check mark (success, approved)
✗ cross (declined, failed, requested changes)
- dash (neutral, draft, pending)
● filled dot (state indicator generic)
○ open dot
```

Use bare ASCII (`v`, `x`, `-`) when `ColorEnabled` is false **and** the locale isn't UTF-8 (`LC_ALL` / `LANG`). Otherwise use Unicode.

Helpers on `ColorScheme`: `success_icon()`, `failure_icon()`, `warning_icon()`, `neutral_icon()`.

## Pager

Post-MVP. The `IoStreams` struct has `start_pager()` / `stop_pager()` no-ops per spec 01. Commands like `bb pr view` and `bb pr diff` should call them around their rendering even now — the stubs are no-ops.

When implemented (likely with `pager` or hand-rolled spawn of `less -R`):
- Resolves pager from `BB_PAGER` → `config.yml` → `$PAGER` → `less -R`.
- Only engages when stdout is a TTY.
- Pipes through, captures errors, restores terminal state on signal (use `tokio::signal::ctrl_c` to clean up).

## Prompts

Use `dialoguer` (mature, widely used, sync API that we wrap in `tokio::task::spawn_blocking` if needed).

```rust
pub trait Prompter: Send + Sync {
    fn input(&self, prompt: &str, default: Option<&str>) -> Result<String, CliError>;
    fn select(&self, prompt: &str, options: &[String], default_idx: usize) -> Result<usize, CliError>;
    fn multi_select(&self, prompt: &str, options: &[String], defaults: &[usize]) -> Result<Vec<usize>, CliError>;
    fn confirm(&self, prompt: &str, default: bool) -> Result<bool, CliError>;
    fn password(&self, prompt: &str) -> Result<String, CliError>;
    fn editor(&self, prompt: &str, initial: &str, ext: &str) -> Result<String, CliError>;
}
```

`Context::prompter` exposes a real `dialoguer`-backed impl. Tests substitute a canned-answer fake.

Rules:
- Never prompt when `!io.is_stdout_tty() || !io.is_stdin_tty()`. Return `CliError::Flag("input required: missing --title flag")` instead.
- Never prompt when `config.yml` has `prompt: disabled`.
- A flag must exist for every prompt — there's always a non-interactive path.

## Exit codes

| Code | Meaning |
| --- | --- |
| 0 | Success |
| 1 | Generic error |
| 2 | Bad arguments (`CliError::Flag`) |
| 3 | Not found (resource doesn't exist) (`CliError::NotFound`) |
| 4 | Authentication required / failed (`CliError::Auth`) |
| 5 | Rate-limited (`CliError::RateLimit`) |
| 6 | Interactively cancelled (`CliError::Cancel`) |

This slice extends `CliError` from spec 01 with `RateLimit { retry_after_secs: u64 }` and `Cancel`. Update `CliError::exit_code()` and `error::report()` accordingly:

```rust
pub fn exit_code(&self) -> u8 {
    match self {
        CliError::Flag(_)         => 2,
        CliError::NotFound(_)     => 3,
        CliError::Auth(_)         => 4,
        CliError::RateLimit { .. } => 5,
        CliError::Cancel          => 6,
        CliError::NotImplemented | CliError::Silent | CliError::Other(_) => 1,
    }
}
```

## Print conventions

- Status messages → `io.err()` (so they don't pollute piped output).
- Actual command output → `io.out()`.
- Success: `✓ Created pull request workspace/repo#42` (green) to stderr.
- Errors: `! error message` (orange) to stderr, then exit.
- Soft warnings during success: `! warning text` to stderr; command still exits 0.

Helpers in `src/cli/messages.rs`:

```rust
pub fn print_notice(io: &mut IoStreams, msg: &str) -> io::Result<()>;
pub fn print_success(io: &mut IoStreams, msg: &str) -> io::Result<()>;
pub fn print_error(io: &mut IoStreams, msg: &str) -> io::Result<()>;
```

All three write to `io.err()`.

## Time formatting

```rust
// src/text.rs (expanded — placeholder file from spec 01)
pub fn rel_time(t: time::OffsetDateTime) -> String;   // "about 2 hours ago", "3 days ago"
pub fn abs_time(t: time::OffsetDateTime) -> String;   // RFC3339 in UTC

pub fn pluralize(n: i64, singular: &str) -> String;   // "1 commit", "5 commits"
pub fn truncate(s: &str, max: usize) -> String;
pub fn indent(s: &str, prefix: &str) -> String;
```

Use relative time for table cells (`UPDATED` column), absolute time in `--json` output and `bb pr view` "Created" field.

## File layout

```
src/iostreams.rs              # extended with table.rs and ColorScheme (still single file or split)
src/iostreams/
├── mod.rs                    # IoStreams, ColorScheme (re-exports if split)
└── table.rs                  # TablePrinter

src/cli/
├── json_flags.rs             # JsonFlags + validate + JsonMode
├── jq.rs                     # `jaq` wrapper
├── messages.rs               # print_notice / success / error
└── prompter.rs               # Prompter trait + dialoguer impl + mock for tests

src/text.rs                   # rel_time, abs_time, pluralize, truncate, indent
```

The file layout is intentionally pragmatic: when `iostreams.rs` grows past ~500 lines, split into `src/iostreams/mod.rs` + `table.rs` + `color.rs`. Until then, keep it as one file.

## Required dependencies (deferred from spec 01)

```toml
jaq-core      = "1"
jaq-interpret = "1"
jaq-parse     = "1"
jaq-std       = "1"
dialoguer     = "0.11"
terminal_size = "0.4"
unicode-width = "0.2"
time          = { version = "0.3", features = ["serde", "macros", "formatting", "parsing"] }
```

`owo-colors`, `anstream`, `serde`, `serde_json`, `clap` are already pinned.

## Tests

- TTY detection: with `IoStreams::test()` the table renderer emits TSV.
- `NO_COLOR=1` strips color escape sequences.
- `--json` with no value lists available fields and exits 1.
- `--jq '.[].title'` over a list applies the filter correctly.
- Truncation handles multi-byte characters correctly (use `unicode-width`).
- `rel_time` for 30s, 5min, 2h, 3d, 4w, 6mo, 2y.
- `CliError::RateLimit` and `Cancel` map to exit codes 5 and 6.

## Concrete deliverables

1. `src/iostreams/table.rs` (or `table` section of `iostreams.rs`) + tests.
2. `ColorScheme` + helpers (added to iostreams).
3. `src/cli/json_flags.rs` + `JsonMode` validator.
4. `src/cli/jq.rs` wrapper around `jaq`.
5. `src/cli/prompter.rs` + dialoguer-backed impl + mock for tests.
6. `src/cli/messages.rs` print helpers.
7. `src/text.rs` filled in with time, pluralize, truncate, indent (replacing the placeholder from spec 01).
8. `CliError::{RateLimit, Cancel}` variants + updated `exit_code()` + `error::report()`.

## Acceptance criteria

- `bb pr list | cat` (once PR commands ship) emits tab-separated rows with no color and no header.
- `bb pr list` in a TTY emits a colored aligned table.
- `bb pr list --json number,title` emits JSON with only those fields.
- `bb pr list --json` (no value) emits the list of valid fields and exits 1.
- `bb pr list --json number,title --jq '.[] | .title'` emits a stream of titles.
- `NO_COLOR=1 bb pr list` emits no ANSI escape sequences.
- An unauthenticated invocation returns exit code 4.
- A missing PR returns exit code 3.
- A user-cancelled prompt returns exit code 6.

## Open questions

- Whether to ship `--template` in MVP. **Lean: no** — `--json --jq` covers the same ground for shell users. Tera is easy to add later.
- Whether to enable the pager wrapper for `bb pr diff` from day one. **Lean: yes**, since unpaged diffs are user-hostile; implement the pager support in iostreams even if other commands skip pager use.
- `dialoguer` vs `inquire` for prompts. **Lean: dialoguer** — older, fewer churn surprises, smaller dep tree.
