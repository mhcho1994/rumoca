# Draft clarification on MSL #4814

Destination: https://github.com/modelica/ModelicaStandardLibrary/issues/4814
Status: not posted. No new count or attachment is proposed by this draft.

---

Thank you for clarifying the distinction between `L*der(i)=v` and
`der(i)=v/L`. We agree that a zero-valued storage coefficient in an implicit
equation is not, by itself, a defect in the primitive component. A parameter
override can require a different state selection/index reduction, and fixed
initial conditions can also make a particular example inconsistent.

Our later local review found cases where retranslating the source or relaxing
incompatible fixed initial conditions allows the zero-valued configuration to
run. We will therefore not use those override failures as evidence for a
blanket positive lower bound on inductance, capacitance, inertia or mass.

For subsequent model-level reports, we will distinguish explicit source
operations outside their domains from implicit equations that need different
structural treatment. We will also keep component-specific domain questions
separate from language-level error semantics already discussed in
ModelicaSpecification#3941.

The earlier spreadsheets list affected locations, including related
occurrences, rather than independent root causes. We are reconciling them
with the later adjudications before proposing another batch or new total.
