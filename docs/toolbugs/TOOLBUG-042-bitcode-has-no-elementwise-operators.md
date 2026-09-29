# TOOLBUG-042 — bitcode v2 has no element-wise operators

**Status:** fixed (this change).
**Severity:** medium — any model using `.+ .- .* ./ .^` exported an
`Unsupported` node and could not be rebuilt from its artifact.

## What

The DAE has ten arithmetic `BinaryOperator`s: five scalar and five
element-wise (MLS §10.6). `RbcBinaryOp` had only the scalar five, and export
mapped the element-wise ones to `Unsupported("binary operator not in bitcode
v2")`. A strict artifact then failed validation, and import refused it.

Found by routing the whole test suite through `DAE -> RBC -> DAE`
(`rumoca compile --pass none`): about twenty test models failed on exactly
this.

## Fix

`RbcBinaryOp` gains `ElementwiseAdd`, `ElementwiseSubtract`,
`ElementwiseMultiply`, `ElementwiseDivide`, `ElementwisePower`, mapped both
ways, in the text profile (`eadd esub emul ediv epow`), the disassembler and
the Python SDK's printer. Additive: artifacts written before still read.
