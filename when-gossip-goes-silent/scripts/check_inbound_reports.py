#!/usr/bin/env python3
"""Quirk Q1 check (specs/001-rust-rewrite research R10): did any mas-agent-inbound service
ever *receive* an inbound report? createGossipReceiver drops `inboundReport`, so we expect
reportsEmitted > 0 but reportsReceived == 0 in every result file.

Usage: python3 scripts/check_inbound_reports.py [results_dir ...]
"""
import glob
import json
import os
import sys

dirs = sys.argv[1:] or sorted(d for d in glob.glob('results*') if os.path.isdir(d))
files = cells = emitted = received = inflations = 0
for d in dirs:
    for f in glob.glob(os.path.join(d, '*.json')):
        with open(f, encoding='utf-8') as fh:
            data = json.load(fh)
        hit = False
        for m in (data.get('serviceMetrics') or {}).values():
            st = (m or {}).get('strategyStats') or {}
            if st.get('strategy') != 'mas-agent-inbound':
                continue
            hit = True
            inb = st.get('inbound') or {}
            emitted += inb.get('reportsEmitted', 0)
            received += inb.get('reportsReceived', 0)
            inflations += inb.get('selfInboundInflations', 0)
        files += 1
        cells += hit
print(f"scanned {files} files in {dirs}; {cells} contain mas-agent-inbound services")
print(f"reportsEmitted={emitted} reportsReceived={received} selfInboundInflations={inflations}")
if cells:
    print("Q1 CONFIRMED: reports are sent but never received" if received == 0 and emitted > 0
          else "Q1 NOT confirmed")
