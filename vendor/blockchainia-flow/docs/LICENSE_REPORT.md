# License and release report

Target project license: `MIT-0`.

Status: **private alpha — public release blocked**.

## Current findings

- The extracted runtime pallet came from an Eterra workspace declaring MIT-0.
- The inspected legacy compiler/core package declared Apache-2.0.
- The new compiler/core/SDK/builder implementation is recorded as a clean-room
  rewrite against locked public wire behavior, not a relicense of that package.
- Third-party dependencies retain their own licenses.

## Required independent approval

Public Git hosting, crate/npm publication, or public builder deployment requires
an independent reviewer to:

1. validate the Eterra pallet's copyright and MIT-0 provenance across history;
2. confirm the clean-room boundary and absence of copied Apache implementation;
3. generate and audit complete Cargo/npm transitive license inventories;
4. confirm trademark/product naming and repository URL;
5. approve all required notices.

This repository deliberately does not mark those items complete. The product
owner's implementation authorization is not treated as legal approval.

