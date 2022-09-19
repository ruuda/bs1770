# Changelog

## Unreleased

**Compatibility**:

 * `gated_mean` now returns `Option<Power>` instead of `Power`. It now returns
   `None` when the mean is undefined (because no signal passes the gate).
   Previously it would return `Power(NaN)` in that case. One way of addressing
   this change is to append `.unwrap_or(Power(0.0))` to calls, which defines
   the gated mean to be zero power (-∞ LKFS) for the empty case.

## 1.0.0

Released 2020-09-02.

Release highlights:

 * Initial release.
 * Support for BS.1770-4 integrated loudness measurement.
 * Supports Rust 1.45.2.
