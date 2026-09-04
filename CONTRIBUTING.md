# Contributing

Thanks for looking. wisp is a GPU user interface toolkit in Rust, built around
the window every other toolkit treats as an afterthought: transparent, always
on top, click-through, and moving with its content.

**Read [Where this is](README.md#where-this-is) first.** Drawing and text work. Layout, input and — the thing it is
named for — the overlay itself do not exist yet, so an issue about missing
layout is a roadmap question rather than a bug.

## Where to start

- **A question, or an idea** — the [Discussions tab](https://github.com/desFernan/wisp/discussions).
- **A bug** — a failing test is the best possible report. Most of this
  library's history is bugs that stayed invisible until something read the
  pixels back, so a test that renders and asserts on the result is worth more
  than a description of what looked wrong.

The design is opinionated in ways that look like mistakes until you hit the
case they are for: fractional positions, `Points` and device pixels as
distinct types, mixing in Oklab, premultiplied alpha throughout. The README
says why for each. Open an issue before changing one of those — they are load-
bearing.

## Build and test

```sh
cargo test --workspace
cargo clippy --workspace --all-targets
cargo run --example cards -p wisp
```

`wisp-core` has no GPU and no platform in it, so nearly all of its tests run
anywhere and in milliseconds. The renderer tests need a GPU adapter: they draw
and read the pixels back, which is the only way most of these bugs are visible
at all.

## Commits and pull requests

Commit subjects here read as one imperative sentence saying what changes for
the person using it — "Let the pet come home to the island", not "refactor
island handling". Some carry a conventional-commit prefix (`chore:`, `fix:`)
and many do not; both are in the history. Match what you see there.

Keep a pull request to one change. If you find a second thing on the way, a
second PR is easier to review and easier to revert.

Run `cargo test --workspace` and `cargo clippy --workspace --all-targets`
before you open it. CI runs them again, but finding it yourself is faster than
a round trip.

## License

MIT for the source — see [LICENSE](LICENSE). By contributing you agree your work is released
under it.
