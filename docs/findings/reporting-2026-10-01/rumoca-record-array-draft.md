# Draft: vectorized scalar-record function call loses array index

Confirmed destination: local `Arslan8/rumoca` fork, `rumoca-bitcode-v1`.
Upstream `CogniPilot/rumoca` attribution: pending clean-upstream reproduction.
Status: not posted.

```modelica
package T
  record Cx
    Real re;
    Real im;
  end Cx;
  function cabs
    input Cx c;
    output Real y;
  algorithm
    y := sqrt(c.re*c.re + c.im*c.im);
  end cabs;
  model M
    parameter Integer m = 2;
    Cx v[m];
    Real a[m] = cabs(v);
    Real x(start=1);
  equation
    der(x) = -x;
    for i in 1:m loop
      v[i].re = x*i;
      v[i].im = 0;
    end for;
  end M;
end T;
```

```sh
rumoca compile T.mo --model T.M --pass none --emit-bitcode T.rbc
```

Expected: vectorize the call over the record array and bind `a[i]` from
`cabs(v[i])`. Actual: compilation returns 1 with ED008, unresolved Flat
reference `v.re`, at the `cabs(v)` call.

Reproduced October 1 on the dirty local build based on
`73c6a7d588cc2d8af04764089edc6910f26e0aa0`. The exact tested source and outputs
are in [compiler-results.json](compiler-results.json). The older report locates
the first divergence in flattening's record-argument decomposition; the fresh
run confirms the unresolved reference, not a complete current phase audit.
No library dependency is needed.
