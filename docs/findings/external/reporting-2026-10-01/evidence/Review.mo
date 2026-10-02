model ReviewLimit
 Buildings.Controls.OBC.CDL.Reals.LimitSlewRate dut(raisingSlewRate=1);
 equation dut.u=time; end ReviewLimit;
 model ReviewRamp
 Buildings.Controls.OBC.CDL.Reals.Ramp dut(raisingSlewRate=1);
 equation dut.u=time; dut.active=true; end ReviewRamp;