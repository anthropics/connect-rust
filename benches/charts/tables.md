| Concurrency | connectrpc-rs | tonic |
|---|---:|---:|
| c=16 | 170,292 | 168,811 (−1%) |
| c=64 | 238,498 | 234,304 (−2%) |
| c=256 | 252,000 | 247,167 (−2%) |

| Concurrency | connectrpc-rs | tonic |
|---|---:|---:|
| c=16 | 32,257 | 28,110 (−13%) |
| c=64 | 73,313 | 68,690 (−6%) |
| c=256 | 112,027 | 84,171 (−25%) |

| Benchmark | connectrpc-rs | tonic |
|---|---:|---:|
| unary_small | 87.6 | 170.8 (+95%) |
| unary_logs_50 | 195.0 | 338.5 (+74%) |
| client_stream | 166.1 | 223.8 (+35%) |
| server_stream | 109.8 | 110.1 (+0%) |
