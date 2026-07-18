# Architecture CI ratchets

Run the complete fast gate locally:

```bash
python3 scripts/check_architecture.py --self-test
```

The same command runs before iOS compilation in pull-request, branch, and
TestFlight test jobs.

It enforces:

- every production Swift file has exactly one ownership entry;
- migrating/temporary owners have implementation issues and deletion targets;
- native presentation does not gain new direct durable-store/runtime access;
- current direct-access exceptions are exact, used, and issue-linked;
- source files never exceed 500 lines;
- existing 300–500-line source is reported and cannot grow;
- any reduction in a soft-baseline file must ratchet its recorded ceiling down;
- new source must start below 300 lines;
- local/vendored generic NMP crates cannot acquire Pod0 product nouns.

The file-length baseline is
[`file-length-baseline.json`](file-length-baseline.json). It is not a license to
grow current files. It makes existing debt visible while allowing incremental
work to split files instead of requiring a blocking repository-wide rewrite.

Do not weaken a checker or add an exception to make a change pass. Change the
architecture, split the file, or link a reviewed migration/deletion decision.
