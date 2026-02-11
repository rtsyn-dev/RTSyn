- **period_us**: Total time between process calls (actual period)
- **latency_us**: Only measures delays (when > expected period) - always ≥ 0
- **jitter_us**: Standard deviation of recent periods - measures timing variation (both early/late)
- **realtime_violation**: 1.0 if latency exceeds max_latency_us threshold
- **max_period (<unit>)**: Maximum observed period since startup, with unit selected by `period_unit`

Variables:
- `max_latency_us`: Violation threshold in microseconds
- `period_unit`: Unit used by `max_period` output (`ns`, `us`, `ms`, `s`)
