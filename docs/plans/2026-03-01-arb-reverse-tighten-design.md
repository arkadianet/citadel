# Arb Scanner Reverse-Tighten Precision

## Problem

The arb scanner's ternary search finds the profit-maximizing input, then `quote_route` forward-propagates through each hop using `calculate_output` (integer division, floors result). This over-estimates the input needed per hop because:

- Forward: input 3.6973 ERG → floor(...) = 505,212 kushti
- Reverse: 505,212 kushti only requires ceil(...) = 3.6874 ERG
- Gap: ~0.01 ERG wasted per hop, accumulates across 4 hops

User confirmed this in practice: scanner said 3.6973 ERG for hop 1, user executed for 3.6874 ERG and got the same output.

## Solution

After ternary search finds the optimal cycle and forward-calculates the route, reverse-tighten each hop's input using `calculate_input`.

### Algorithm

1. Ternary search finds optimal input, `quote_route` gives forward route with per-hop amounts
2. Start from last hop's output amount
3. Walk backwards: for hop N, call `calculate_input(reserves_in, reserves_out, hop_output, fee_num, fee_denom)` to get exact minimum input
4. That minimum input = output of hop N-1
5. Continue to hop 1
6. Hop 1's tightened input = new optimal_input (less than forward-calculated)
7. Recalculate gross/net profit against tightened input

### Edge case

`calculate_input` returns `None` if output >= reserves. Fall back to forward-calculated route.

## Changes

**Rust** (`crates/protocols/amm/src/router.rs`):
- New function `tighten_cycle_inputs` called inside `find_circular_arbs` after `quote_route`
- Updates `CircularArb` fields: `optimal_input_nano`, `gross_profit_nano`, `net_profit_nano`, `profit_pct`, `route`

**Frontend**: No changes. Existing UI reads per-hop amounts from `route.hops`, which become more precise automatically.
