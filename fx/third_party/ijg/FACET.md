Facet adapts the integer forward and inverse transforms from IJG release 6b into Rust in core/src/batch4/jpeg_dct.rs. The loops are factored into one-dimensional passes, wide intermediates are used, and the inverse accepts already dequantized i32 values. No entropy codec is included. Source: https://www.ijg.org/files/jpegsrc.v6b.tar.gz

This software is based in part on the work of the Independent JPEG Group. See the unmodified README for copyright, license and disclaimer.
