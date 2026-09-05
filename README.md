# disc0

**What is using my disk, what grew, who owns it, and what happens if I remove it.**

A local tool for humans and agents. One command, no project integration, no model, no cloud
account, no inference cost. Readable terminal output by default; the same evidence and the same
decisions as stable JSON for machines.

> **v0.1 is READ-ONLY.** `disc0` never modifies or removes any file outside its own state
> directory. Cleanup is a later milestone, deliberately gated behind a durability contract that
> is not finished yet — see [Why cleanup isn't here yet](#why-cleanup-isnt-here-yet).

```
$ disc0 scan ~/Projects

  /home/dev/Projects   1.9 GiB in 4587 files   (1.9 GiB logical)
  scanned in 23ms · 5186 entries

  FINDING                          ALLOCATED  OWNER                        CONSEQUENCE
  app/target                         1.9 GiB  cargo_project (corroborated) conditional_rebuild
  web/node_modules                 279.7 MiB  node_project (corroborated)  requires_download
  esc/target                       352.0 KiB  cargo_project (corroborated) conditional_rebuild
    ↳ reclaimable unknown: inodes in this subtree have hard links outside it;
      removing this path alone does not free their blocks
  decoy/target                     204.0 KiB  unknown (weak)               unknown
```

---

## What makes it different

Size rankers already exist and are good at it. `disc0` is not trying to be a faster one. It
answers the questions a size ranker cannot:

**Who owns this, and how do you know?** Every finding carries the facts that produced its
classification — the manifest that was found, the build layout that corroborated it — plus a
qualitative evidence level (`corroborated` / `partial` / `weak`). Not an invented confidence
percentage.

**What happens if I remove it?** `conditional_rebuild`, `requires_download`,
`potentially_unique`, or `unknown` — with the recovery prerequisites spelled out. A lockfile does
not guarantee the upstream packages still exist, and `disc0` says so.

**What did I decide last month, and why?** Scan history is persisted in
[NEDB](https://github.com/Eth-Interchained/nedb), a content-addressed hash-chained store. Every
finding cites the observations that justified it, so the reasoning is reconstructable rather than
remembered.

**A directory name never authorizes anything.** A folder called `target` holding tracked source
stays `unknown`, forever, no matter how big it is.

---

## Honest accounting

Most of the work here is refusing to state a number that isn't true.

- **Logical and allocated size are tracked separately.** They differ a lot: one real 34,630-file
  tree measured 216.5 MiB logical and 293.5 MiB allocated. Block rounding is not a rounding error.
- **Hard links are counted once**, by filesystem identity. Two links to one inode is one
  allocation, not two.
- **Reclaimable bytes are `null` when they are unknowable.** If an inode in a subtree has links
  from outside it, deleting that subtree frees nothing, and the field reports `unknown` with the
  reason attached. Sparse files, compression, reflinks, snapshots and open-deleted files all
  prevent a universally exact answer, and `disc0` will not invent one.
- **Parent/child selections are de-duplicated** — the same blocks are never summed twice.
- **Partial coverage is loud.** Permission denials, files that vanish mid-scan, and mount
  boundaries are recorded as first-class records; the totals are then explicitly labelled a lower
  bound and the exit code says so.
- **Symlinks are recorded, never followed.** The scan stops at mount boundaries unless you ask
  otherwise.
- **`mtime` is not last-use time**, and is never used to infer that anything is unused.

---

## Usage

```bash
disc0 scan <path> [--json] [--quiet] [--cross-filesystems] [--ephemeral] [--limit N]
disc0 findings [--json]
disc0 explain <finding-id>
disc0 status [--json]
```

A scan reports live: a throttled status line while walking, then a timed phase per stage.

```
  ⠙     18432 entries ·    16204 files ·   142.1 MiB ·   61440/s   /Users/…/node_modules/.pnpm
  ✓ SCAN 39400 entries · 34630 files · 293.5 MiB (70ms)
  ✓ DETECT 1 findings (33ms)
  ✓ PERSIST 40 observation pages · 1 findings · durable (130ms)
```

All of it goes to **stderr**, so stdout carries only the report or the JSON document.
`--quiet` silences it; `--json` silences it automatically and emits nothing on stderr at all.

`--ephemeral` runs entirely in memory: nothing is saved, no baseline is created for future
comparison, and it tells you that rather than leaving you to assume.

### Exit codes

| code | meaning |
|---|---|
| 0 | complete, successful operation |
| 1 | operational error |
| 2 | incomplete scan, or a stale / untrustworthy baseline |
| 3 | user cancellation |

`--json` never implies approval for anything. Machine output goes to stdout; progress goes to
stderr, so a JSON document is never polluted by a status line.

### `explain` — the five facts, in order

```
$ disc0 explain scan_1788641541_15007_f2

  /tmp/fixt/esc/target
  352.0 KiB allocated in 2 files

  OWNER      cargo_project — evidence: corroborated
             project root /tmp/fixt/esc
             · cargo manifest present at /tmp/fixt/esc/Cargo.toml
             · cargo build layout present: debug, CACHEDIR.TAG

  IF REMOVED conditional_rebuild
             Next build regenerates these outputs. Requires the toolchain and
             dependencies; artifacts from an unavailable build environment may
             not be reproducible.
             reclaimable bytes UNKNOWN: one or more inodes in this subtree have
             hard links outside it; removing this path alone does not free them.

  CLEANUP    eligible=false — v0.1 is read-only; no cleanup is performed

  RECEIPT    6 records in the causal chain
             findings/scan_1788641541_15007_f2
             observation_pages/scan_1788641541_15007_p0
             scans/scan_1788641541_15007
```

---

## Detectors

Each returns a category, owner candidates, evidence, a consequence, and a rule version.
Manifests are parsed as **inert data** — no build script is ever executed to classify a
directory.

| detector | corroborating evidence | downgraded when |
|---|---|---|
| Cargo build output | `Cargo.toml` in the parent plus a real build layout | custom target dir, tracked source inside, `.git` present |
| Node dependencies | `package.json` plus a lockfile | no lockfile (restoration not reproducible) |
| Python bytecode | `__pycache__` beside matching `.py` sources | bytecode-only distribution |
| Virtual environment | `pyvenv.cfg` plus a dependency manifest | no manifest — requirements unknowable from here |
| Model files | `.gguf` / `.safetensors` | extension only; origin unverified, so never actionable |

"Not referenced by the metadata we checked" is never reported as "unused."

---

## Storage: why observations are packed

`disc0` records every filesystem entry it saw so a later scan can explain what grew. The obvious
shape — one database document per file — does not work, and the measurement is worth stating
because it is counter-intuitive.

Measured on a real 39,400-entry tree holding 293 MB of `node_modules`:

| observation model | wall | state on disk | objects |
|---|---|---|---|
| one document per file | 4,725 ms | **323 MB** | 39,403 |
| one document per file, v3 segments | 3,860 ms | 185 MB | 39,403 |
| **packed pages of 1,000** | **292 ms** | **11 MB** | **43** |

The cause is not the data. Each document costs an object file plus an id-index leaf, and every
file rounds up to a filesystem block — an id-index leaf is a 64-byte hash string occupying 4096
bytes. 39,403 documents × 4096 × 2 lands exactly on the 323 MB measured. Packing 1,000
observations per document removes the per-document overhead and amortizes the node envelope,
giving 16× the speed and 29× less state.

A tool that explains disk consumption while consuming more disk than it explains fails at its own
purpose. Retention of 10 scans of that tree is 114 MB packed, against 3.2 GB unpacked.

**The cost, stated plainly:** a receipt now points at an observation *page* rather than a single
file record. The evidence is still verbatim, still content-addressed, still tamper-evident — but
`TRACE` lands on a page and you locate the exact file inside it.

---

## Why cleanup isn't here yet

Recognized does not mean disposable, and a plan is not permission.

Deletion is permanent and has no automatic undo, so it ships only behind a chain that is fully
built and tested: plans that pin exact file identities, revalidation at apply time, explicit
authorization that a `--json` flag cannot grant, argv executed directly rather than interpolated
into a shell, and a durable intent recorded *before* mutation so an interrupted cleanup leaves an
honest receipt instead of a guess.

That chain depends on the storage layer being able to report a durability failure. Through
`nedb-engine` 2.8.5 it could not — `flush_all()` returned no value and swallowed a failed
`fsync`, and a flush that hit a full disk silently discarded acknowledged writes. Both are fixed
in **2.8.6** (`try_flush_all()`, and failed index writes retained for retry), which is why disc0
pins that version exactly. Until every gate above is built and adversarially tested, `disc0`
stays read-only and says so.

---

## Install

Requires a Rust toolchain (Linux and macOS; Windows follows once filesystem identity and
deletion guarantees are verified there).

```bash
git clone https://github.com/Eth-Interchained/disc0
cd disc0
cargo build --release
./target/release/disc0 scan ~/Projects
```

State lives under `$DISC0_STATE`, else `$XDG_STATE_HOME/disc0`, else `~/.local/state/disc0`.
It is always excluded from its own scans. Only metadata is stored — never file contents, never
secrets, and nothing is ever uploaded anywhere.

Only one process may hold a state directory at a time; a second gets a clear refusal naming the
process that holds it, rather than corrupting shared state.

---

## Repository layout

```
crates/disc0        library + CLI — the scanner, detectors, accounting, store
crates/disc0-gui    native GUI (planned) — links the library IN-PROCESS
```

One repo, one lockfile, one `cargo build`. The GUI calls `scan()` directly and gets real structs
back: no subprocess, no JSON round trip, and no way for the CLI and GUI to drift onto different
versions of the scanner.

---

## Status

Working today: scanning, detectors, honest accounting, NEDB-backed history, human and JSON
output, `explain` with the causal receipt, baseline trust verification.

Not built yet: history comparison between scans (`diff`), cleanup planning and execution, Docker
and shared package-manager cache adapters, the GUI, published packages.

No benchmark against other tools, no adoption result, and no recovered-space measurement has been
made. Where a number appears in this README it was measured on a real tree and the tree is
described; nothing here is projected.

---

## License

GPL-3.0-or-later. See [LICENSE](LICENSE).

Built by [Interchained](https://github.com/Eth-Interchained) on
[NEDB](https://github.com/Eth-Interchained/nedb).
