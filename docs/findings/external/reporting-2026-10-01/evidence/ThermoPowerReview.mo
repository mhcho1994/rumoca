model ReviewController
 inner ThermoPower.System system;
 ThermoPower.Test.ElectricalComponents.SecondaryController dut(Pnom=40e6,Ts=300);
 equation dut.frequency=50; end ReviewController;