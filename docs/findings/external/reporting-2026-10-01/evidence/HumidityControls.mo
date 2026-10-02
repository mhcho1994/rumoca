model DewPointControl
 extends Buildings.Controls.OBC.CDL.Psychrometrics.Validation.DewPoint_TDryBulPhi(phi(height=0.999));
end DewPointControl;
model EnthalpyControl
 extends Buildings.Controls.OBC.CDL.Psychrometrics.Validation.SpecificEnthalpy_TDryBulPhi(phi(height=0.999));
end EnthalpyControl;
