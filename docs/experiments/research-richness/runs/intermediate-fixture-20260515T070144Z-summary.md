# Research Richness Intermediate Fixture Summary

## Purpose

Capture the latest fixture-baseline checkpoint for the research-richness benchmark before live runs and ablation work.

## Command Context

- Mode: fixture baseline
- Run time: 2026-05-15T07:01:44Z
- Scope: headless benchmark execution through the research controller loop
- Input: existing benchmark cases and fixture-mode controller paths

## Result Summary

- 7/7 benchmark cases completed
- Quality passed for all 7 cases
- Source diagnostics present
- Controller artifacts present
- Context packing diagnostics present

## Aggregate Fixture Signals

- source_pack: success
- source_pack queries per case: 1
- adopted sources per case: 7
- controller artifacts per case:
  - source cards: 7
  - claims: 7
  - conflicts: 1
  - open debt: 0
- quality_gate: passed per case

## Provider Smoke State

- naver: HTTP 200
- kakao: HTTP 200
- local provider chain: `naver,kakao,duckduckgo`

## Unresolved Items

- Live source-pack integration
- Context/window ablation knob
