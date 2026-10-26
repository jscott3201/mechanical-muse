# open-control-library fixtures (pre-existing)

Copied unmodified from [open-control-library](https://github.com/jscott3201/open-control-library) at
commit `124427b` (the pin in the README's "Pre-existing dependencies" table), under its MIT OR
Apache-2.0 license. They are test data, not work done during the hackathon.

Each folder holds one fault rule's `rule.cxf.jsonld` and `vectors.json` from `faults/<family>/<ID>/`.

| Rule | Why it is here |
| --- | --- |
| AHU-0027 | Smallest rule (2 blocks); the worked example in `docs/block-graph-schema.md` |
| AHU-0005 | Port order matters (`Reals.Subtract` is `u1 - u2`) |
| HW-0011 | Largest rule (28 blocks, 38 connections) |
| RTU-0007 | Writes the literal `0.30`, so it round-trips by meaning, not byte for byte |
| HP-0005 | Writes a single root output as an object, not an array |
| AHU-0023 | Node order differs from the emitter's |
