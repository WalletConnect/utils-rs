# utils-rs

Rust Utilities for WalletConnect

## `alloc`

Exports `Jemalloc` (from the [`tikv-jemallocator`](https://github.com/tikv/jemallocator)) with service metrics instrumentation. Also contains a custom lightweight version of the [DHAT profiler](https://github.com/WalletConnect/dhat-rs) to automate heap profiling in async environment.

## `collections`

Extensions for collections such as `HashMap`.

## `future`

Convenience `Future` extensions.

## `metrics`

Global service metrics. Currently based on `opentelemetry` SDK and exported in `prometheus` format.

## Examples

- [Metrics integration](examples/metrics.rs). Prints service metrics in the default (`prometheus`) format.
- [Allocation profiler](examples/alloc_profiler.rs). Demonstrates how to set up the DHAT profiler and record a profile of specified allocation bin sizes. Note that in order to get proper stack traces in a `release` build you need to enable debug symbols, e.g. using a custom build profile in `Cargo.toml`:
  ```toml
  [profile.release-debug]
  inherits = "release"
  lto = "thin"
  debug = 1
  ```
- [Allocation stats](examples/alloc_stats.rs). Demonstrates how to set up Jemalloc and instrument allocation stats with service metrics.

## License

[MIT](LICENSE)
