# Command-line reference

<!-- Made from the CLI's own definitions by crates/cli/src/reference.rs. Don't edit this page: change the code, then run `COLDOPEN_BLESS=1 cargo test -p coldopen-cli reference`. -->

`coldopen` comes with the app on every system (see [Install](../guide/install.md)). Every subcommand sends the same commands the app does, so a script is changed the same way from here as from the window. `coldopen --version` prints the version and commit, and `coldopen help <subcommand>` prints a subcommand's help.

| Subcommand | What it does |
| --- | --- |
| [`convert`](#coldopen-convert) | Convert a script to Fountain, .fdx or JSON. |
| [`repair`](#coldopen-repair) | Repair a damaged .fdx file into a new one, and print what was repaired. The input is never written. |
| [`export`](#coldopen-export) | Export a script as a PDF. |
| [`pages`](#coldopen-pages) | Print the pages of a script as plain text, the way they print. |
| [`report`](#coldopen-report) | Print a report: scenes, characters or locations. |
| [`diff`](#coldopen-diff) | Compare two drafts, in any of the three formats, scene by scene. Prints a summary; exits 0 whether or not they differ. |
| [`scenes`](#coldopen-scenes) | List the scenes in a script. |
| [`run`](#coldopen-run) | Run a batch of commands from a JSON file (an array of requests, or one request per line). Prints one response per line. |
| [`licenses`](#coldopen-licenses) | Print the licenses of Cold Open and of the fonts it bundles. |
| [`bench`](#coldopen-bench) | Measure every speed and memory budget on this computer and write a small report to send back. Runs the desktop app installed beside this command; takes about two minutes, during which its windows come and go. |
| [`serve`](#coldopen-serve) | Read JSON requests on standard input, one per line, and answer each on standard output. |
| [`mcp`](#coldopen-mcp) | Serve the Model Context Protocol on standard input and output, so an AI agent can read and edit scripts. When the Cold Open app has the script open, the agent's edits go into the app, live and undoable. |

## `coldopen convert`

Convert a script to Fountain, .fdx or JSON.

```text
coldopen convert [OPTIONS] <INPUT> <OUTPUT>
```

| Argument | What it does |
| --- | --- |
| `<INPUT>` | The script to read (.fountain, .spmd, .txt or .fdx) |
| `<OUTPUT>` | Where to write, or - for standard output. |
| `--to <TO>` | Output format. Defaults to the output file's extension, or Fountain. One of `fountain`, `fdx`, `json`. |
| `--allow-lossy` | Write even if the format cannot hold everything in the script. |

## `coldopen repair`

Repair a damaged .fdx file into a new one, and print what was repaired. The input is never written.

```text
coldopen repair [OPTIONS] <INPUT> <OUTPUT>
```

| Argument | What it does |
| --- | --- |
| `<INPUT>` | The .fdx file to repair. |
| `<OUTPUT>` | Where to write the repaired copy (.fdx) |
| `--json` | Print the report as JSON. |
| `--report <REPORT>` | Also write the report to this file. |

## `coldopen export`

Export a script as a PDF.

```text
coldopen export [OPTIONS] <INPUT> <OUTPUT>
```

| Argument | What it does |
| --- | --- |
| `<INPUT>` | The script to read (.fountain, .spmd, .txt or .fdx) |
| `<OUTPUT>` | The PDF to write. |
| `--watermark <WATERMARK>` | Print this text diagonally across every page. |
| `--scene-numbers` | Print scene numbers beside the scene headings. |
| `--no-title-page` | Leave out the title page. |

## `coldopen pages`

Print the pages of a script as plain text, the way they print.

```text
coldopen pages <FILE>
```

| Argument | What it does |
| --- | --- |
| `<FILE>` | The script to read. |

## `coldopen report`

Print a report: scenes, characters or locations.

```text
coldopen report [OPTIONS] <KIND> <FILE>
```

| Argument | What it does |
| --- | --- |
| `<KIND>` | Which report. One of `scenes`, `characters`, `locations`. |
| `<FILE>` | The script to read. |
| `--format <FORMAT>` | Text for people, the command's JSON, or CSV (RFC 4180) One of `text`, `json`, `csv`. Default: `text`. |

## `coldopen diff`

Compare two drafts, in any of the three formats, scene by scene. Prints a summary; exits 0 whether or not they differ.

```text
coldopen diff [OPTIONS] <A> <B>
```

| Argument | What it does |
| --- | --- |
| `<A>` | The first draft. |
| `<B>` | The second draft. |
| `--json` | Print the comparison as JSON. |
| `--pdf <PDF>` | Also write the second draft with the changes marked to this PDF. |
| `--overwrite` | Replace the PDF if it exists. |

## `coldopen scenes`

List the scenes in a script.

```text
coldopen scenes [OPTIONS] <FILE>
```

| Argument | What it does |
| --- | --- |
| `<FILE>` | The script to read. |
| `--json` | Print the raw scene.list result as JSON. |

## `coldopen run`

Run a batch of commands from a JSON file (an array of requests, or one request per line). Prints one response per line.

```text
coldopen run [OPTIONS] <EDITS>
```

| Argument | What it does |
| --- | --- |
| `<EDITS>` | The file of command requests, or - for standard input. |
| `--doc <DOC>` | Open this script before running the commands. |
| `--save` | Save the script after the commands succeed. |
| `--keep-going` | Keep going after a command fails. |
| `--author <AUTHOR>` | The name your tracked changes carry (default "Writer") |

## `coldopen licenses`

Print the licenses of Cold Open and of the fonts it bundles.

```text
coldopen licenses
```

## `coldopen bench`

Measure every speed and memory budget on this computer and write a small report to send back. Runs the desktop app installed beside this command; takes about two minutes, during which its windows come and go.

```text
coldopen bench [OPTIONS]
```

| Argument | What it does |
| --- | --- |
| `--out <OUT>` | Where to write the report (default: your Downloads folder) |
| `--quick` | Fewer repetitions, for a quick check; not for judging budgets. |

## `coldopen serve`

Read JSON requests on standard input, one per line, and answer each on standard output.

```text
coldopen serve [OPTIONS]
```

| Argument | What it does |
| --- | --- |
| `--stdio` | Use standard input and output (the only transport so far) |
| `--author <AUTHOR>` | The name your tracked changes carry (default "Writer") |

## `coldopen mcp`

Serve the Model Context Protocol on standard input and output, so an AI agent can read and edit scripts. When the Cold Open app has the script open, the agent's edits go into the app, live and undoable.

```text
coldopen mcp [OPTIONS]
```

| Argument | What it does |
| --- | --- |
| `--doc <DOC>` | A script to open, or attach to, before the first request. |
| `--no-attach` | Never attach to the app; always open the file in this process. |
| `--untracked` | Do not record the agent's edits as tracked changes in scripts this server opens itself. When the script is open in the app, the app's setting decides. |
