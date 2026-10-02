from pathlib import Path
import subprocess,json,concurrent.futures,os
root=Path('/data/mrumoca/rumoca'); out=root/'target/external-reporting-review-20261001'
configs={
'buildings':('/data/mrumoca/rumoca/target/msl/ModelicaStandardLibrary-4.1.0', 'Modelica 4.1.0','ModelicaServices 4.1.0', '/data/mrumoca/eval/_repos/buildings/Buildings/package.mo', '''model ReviewLimit
 Buildings.Controls.OBC.CDL.Reals.LimitSlewRate dut(raisingSlewRate=1);
 equation dut.u=time; end ReviewLimit;
 model ReviewRamp
 Buildings.Controls.OBC.CDL.Reals.Ramp dut(raisingSlewRate=1);
 equation dut.u=time; dut.active=true; end ReviewRamp;''',['ReviewLimit','ReviewRamp','Buildings.HeatTransfer.Windows.BaseClasses.Validation.HeatCapacityConstantShade']),
 'thermopower':('/data/mrumoca/eval/_msl/msl-3.2.3','Modelica','ModelicaServices','/data/mrumoca/eval/_repos/thermopower/ThermoPower/package.mo', '''model ReviewController
 inner ThermoPower.System system;
 ThermoPower.Test.ElectricalComponents.SecondaryController dut(Pnom=40e6,Ts=300);
 equation dut.frequency=50; end ReviewController;''',['ReviewController'])}
def run(item):
 name,(base,mod,services,lib,source,models)=item
 d=out/name;d.mkdir(exist_ok=True)
 (d/'Review.mo').write_text(source)
 lines=['setCommandLineOptions("-d=frontEndUnitCheck");','setEnvironmentVar("CC","gcc");','setEnvironmentVar("CXX","g++");']
 for f in [f'{base}/{services}/package.mo',f'{base}/Complex.mo',f'{base}/{mod}/package.mo',lib,str(d/'Review.mo')]: lines.append(f'loadFile({json.dumps(f)}); getErrorString();')
 for m in models:lines += [f'print("MODEL: {m}\\n");',f'checkModel({m});','getErrorString();']
 (d/'check.mos').write_text('\n'.join(lines)+'\n')
 p=subprocess.run(['omc',str(d/'check.mos')],cwd=d,text=True,capture_output=True,timeout=120,env={**os.environ,'CC':'gcc','CXX':'g++'})
 (d/'output.txt').write_text(p.stdout+p.stderr)
 return {'library':name,'returncode':p.returncode,'output':p.stdout+p.stderr,'script':str(d/'check.mos')}
with concurrent.futures.ThreadPoolExecutor(max_workers=2) as pool:
 results=list(pool.map(run,configs.items()))
(out/'unit-check-results.json').write_text(json.dumps(results,indent=2)+'\n')
for r in results:print(r['library'],r['returncode'],r['output'][-6000:])
