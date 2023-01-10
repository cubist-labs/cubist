We include two html files when generating docs with `rust doc` to syntax
highlight Solidity source files:

- in-header.html: loads highlight.js and solidity.min.js
- after-content.html: applies the highilght to all solidity code blocks


There are two ways to tell `rust doc` to do this. First, via the environment:

```
RUSTDOCFLAGS="--html-in-header utils/rustdoc-syntax-highlighting/in-header.html --html-after-content utils/rustdoc-syntax-highlighting/after-content.html" cargo doc --no-deps
```

Second, via a `.cargo/config.toml`:

```toml
[build]
# load highlight.js to syntax highlight solidity files
rustdocflags = [
  "--html-in-header", "utils/rustdoc-syntax-highlighting/in-header.html",
  "--html-after-content", "utils/rustdoc-syntax-highlighting/after-content.html"
]
```

You probably don't want to use a config file unless you never plan to build the
docs for dependencies.
