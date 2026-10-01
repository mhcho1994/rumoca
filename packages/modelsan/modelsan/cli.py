"""ModelSan command line.

    modelsan check Model.mo --model M -fsanitize=default   sanitize one model
    modelsan contracts campaign.json --backend ...          behavioural contracts
"""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys

from .backends.openmodelica import OpenModelicaBackend
from .backends.rumoca import RumocaBackend
from .backends.rumoca_source import RumocaSourceBackend
from .campaign import run_campaign
from .campaign_provenance import capture
from . import check as check_command


def main(argv=None):
    parser = argparse.ArgumentParser(description='ModelSan: sanitizers for Modelica models')
    commands = parser.add_subparsers(dest='command', required=True)
    contracts = commands.add_parser('contracts', help='check declared properties of actual executions')
    contracts.add_argument('campaign', type=Path)
    contracts.add_argument('--backend', required=True, choices=['rumoca','rumoca-source','openmodelica'])
    contracts.add_argument('--rumoca', default='./target/debug/rumoca')
    contracts.add_argument('--source-root', action='append', default=[])
    contracts.add_argument('--library', action='append', default=[], help='Modelica package/file loaded by OMC')
    contracts.add_argument('--cache-dir')
    contracts.add_argument('--freeze-parameters', action='store_true',
        help='native fixed-input compilation; parameter overrides require recompilation')
    contracts.add_argument('--stop-time', type=float, default=1.0)
    contracts.add_argument('--intervals', type=int, default=400)
    contracts.add_argument('--timeout', type=float, default=90.0)
    contracts.add_argument('--output', type=Path, required=True)
    check_command.add_parser(commands)
    args = parser.parse_args(argv)
    if args.command == 'check':
        return check_command.run(args)
    if args.intervals < 1 or args.stop_time <= 0:
        parser.error('intervals and stop-time must be positive')
    if args.backend != 'openmodelica' and args.library:
        parser.error('--library applies to OpenModelica; native source uses --source-root')
    if args.backend == 'openmodelica' and (args.source_root or args.cache_dir or args.freeze_parameters):
        parser.error('--source-root/--cache-dir/--freeze-parameters apply to native backends')

    def backend():
        if args.backend == 'rumoca':
            return RumocaBackend(args.rumoca, t_end=args.stop_time, timeout=args.timeout,
                source_roots=args.source_root, cache_dir=args.cache_dir,
                dt=args.stop_time/args.intervals, freeze_parameters=args.freeze_parameters)
        if args.backend == 'rumoca-source':
            return RumocaSourceBackend(args.rumoca, t_end=args.stop_time, timeout=args.timeout,
                source_roots=args.source_root, cache_dir=args.cache_dir,
                dt=args.stop_time/args.intervals, freeze_parameters=args.freeze_parameters)
        return OpenModelicaBackend(args.library, t_end=args.stop_time, timeout=args.timeout,
                                   number_of_intervals=args.intervals)

    try:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        report = run_campaign(args.campaign, backend, args.output,
                              progress=lambda name: print(f'Checking {name}', file=sys.stderr, flush=True),
                              provenance=capture(args))
    except (OSError, ValueError, KeyError, TypeError) as error:
        parser.error(str(error))
    summary = {case['model']: case['status'] for case in report['cases']}
    print(json.dumps(summary, indent=2))
    if any(status in ('blocked','unobserved','inconclusive') for status in summary.values()):
        return 2
    return 1 if any(status != 'no-violation-observed' for status in summary.values()) else 0


if __name__ == '__main__':
    raise SystemExit(main())
