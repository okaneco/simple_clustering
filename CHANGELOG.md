# `simple_clustering` changelog

## Version 0.2.0 - 2023-07
Updated the color-handling crate, `palette`, from `0.6` to `0.7`. Users will
need to change from using `palette::Pixel::from_raw_slice` to
`palette::cast::from_component_slice` for preparing the input image buffer.

Consult the [documentation] or `lib.rs` file for examples.

[#3][3] - Upgrade dependencies, update CI/CD workflows, bump to 0.2.0

## Version 0.1.1 - 2023-01
Improved SLIC calculation speed by ~15-20% after refactoring calculation loop.

[#1][1] - Bump to 0.1.1, Optimize slic index computation

## Version 0.1.0 - 2022-04
- Initial Commit

[documentation]: https://docs.rs/simple_clustering
[1]: https://github.com/okaneco/simple_clustering/pull/1
[3]: https://github.com/okaneco/simple_clustering/pull/3
