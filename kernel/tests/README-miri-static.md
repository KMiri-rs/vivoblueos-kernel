# Static publication diagnostic

These no_std/non-test diagnostic sources are run explicitly, not auto-discovered Cargo integration tests.
Build the kernel boot dependencies using the companion build runner, then run:

```bash
python3 build/scripts/miri/run_boot_fixture.py out/miri-boot/boot-normal/evidence/boot-normal-command.json kernel/kernel/tests/miri_static_publication.rs
```

The full original startup diagnostics and clean-workspace reproduction instructions are in the companion build PR at `scripts/miri/blueos_boot/README.md`.
