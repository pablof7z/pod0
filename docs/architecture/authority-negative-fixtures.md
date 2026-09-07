# Authority negative fixtures

The architecture self-tests inject each prohibited authority bypass and require
the validator to return the exact rule ID below. A fixture passing silently or
failing for a different reason is a test failure.

| Prohibited bypass | Rule ID | Exercised by |
| --- | --- | --- |
| Unregistered typed input | `unregistered_input` | `check_activity_conformance.py --self-test` |
| Arbitrary transition mutation closure | `arbitrary_mutation_closure` | `check_activity_conformance.py --self-test` |
| Wildcard command/effect/observation route | `wildcard_routing` | `check_activity_conformance.py --self-test` |
| Native default, fallback, or retry decision | `native_default_fallback_retry` | `check_rust_business_logic_boundary.py --self-test` |
| Fabricated canonical activity | `semantic_fact_construction` | `check_rust_business_logic_boundary.py --self-test` |
| In-memory-only effect authorization | `in_memory_only_authorization` | `check_rust_business_logic_boundary.py --self-test` |
| Acceptance of a stale observation | `stale_observation_acceptance` | `check_rust_business_logic_boundary.py --self-test` |
| Restoration of a retired native writer | `restored_retired_writer` | `check_rust_business_logic_boundary.py --self-test` |

Run both fixture sets and their live repository checks:

```sh
python3 scripts/check_activity_conformance.py --self-test
python3 scripts/check_activity_conformance.py
python3 scripts/check_rust_business_logic_boundary.py --self-test
python3 scripts/check_rust_business_logic_boundary.py
```
