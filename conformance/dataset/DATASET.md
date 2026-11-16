# Core's test metas through the SDKs

Written by `shapekit conformance run --dataset`. 431546 metas core wrote in its transaction tests (stellar/unsafe-stellar-core@sha256:a8cb428ce497695321fdffc8cdb23b8102c1074ebe480f1c9e923fe1052e22ff), 28142 distinct, in 29691 test sections, each distinct meta through the SDK's decode and encode.

| testee | distinct | round-trip | fail | beyond max_protocol | sections passing |
|---|---|---|---|---|---|
| rust@30.0.0 | 28142 | 28142 | 0 | 0 | 29691 of 29691 |
| go@v0.9.0 | 28142 | 28142 | 0 | 0 | 29691 of 29691 |
| js@17.4.0 | 28142 | 28142 | 0 | 0 | 29691 of 29691 |
