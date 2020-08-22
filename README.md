# BS1770

A Rust library that implements ITU-R BS.1770-4 loudness measurement.

## Example

TODO

## Performance

The initial focus is on correctness, the library has not been optimized yet.
There is a lot of potential for optimization, for example by combining filters,
unrolling loops, applying vectorization, etc.

## References

 * [ITU-R BS.1770-4][bs1770], a standard that specifies how to measure loudness,
   and which defines the LKFS unit (loudness units full scale, K-weighted).
 * [ITU-R BS.1771-1][bs1771] builds upon BS.1770 with a few requirements for
   building loudness meters.
 * [EBU R 128][r128], which specifies a target loudness level, based on the
   BS.1770 loudness measurement.
 * [EBU Tech 3341][tech3341], which specifies “EBU Mode” loudness meters, but
   which in particular provides test vectors to confirm that a meter implements
   BS.1770 correctly. It also proposes to move away from the term “LKFS”
   introduced in BS.1770, in favor of the term “LUFS”. K-weighting would be
   indicated elsewhere.
 * [EBU Tech 3342][tech3342], which specifies how to measure loudness range.

[bs1770]:   https://www.itu.int/rec/R-REC-BS.1770-4-201510-I/en
[bs1771]:   https://www.itu.int/rec/R-REC-BS.1771-1-201201-I/en
[r128]:     https://tech.ebu.ch/publications/r128
[tech3341]: https://tech.ebu.ch/publications/tech3341
[tech3342]: https://tech.ebu.ch/publications/tech3342

## Acknowledgements

 * The filter coefficient formulas are adapted from [pyloudnorm][pyloudnorm] by
   Christian Steinmetz.
 * The filter coefficient formulas are [originally due to Brecht De Man][deman],
   but the associated paper is not openly accessible.

[pyloudnorm]: https://github.com/csteinmetz1/pyloudnorm
[deman]:      https://github.com/BrechtDeMan/loudness.py

## License

BS1770 is licensed under the [Apache 2.0][apache2] license. It may be used in
free software as well as closed-source applications, both for commercial and
non-commercial use under the conditions given in the license. If you want to
use BS1770 in your GPLv2-licensed software, you can add an [exception][except]
to your copyright notice. Please do not open an issue if you disagree with the
choice of license.

[apache2]: https://www.apache.org/licenses/LICENSE-2.0
[except]:  https://www.gnu.org/licenses/gpl-faq.html#GPLIncompatibleLibs
