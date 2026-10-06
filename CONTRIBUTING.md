# Contributing to Dolfin

Thank you for your interest in contributing to **Dolfin**, an indentation-based ontology definition language that compiles to OWL/RDF.

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Getting Started](#getting-started)
- [How to Contribute](#how-to-contribute)
- [Submitting Changes](#submitting-changes)
- [Coding Standards](#coding-standards)
- [Reporting Bugs](#reporting-bugs)
- [Suggesting Features](#suggesting-features)

---

## Code of Conduct

We expect all contributors to be respectful and constructive. Harassment, discrimination, or hostile behavior of any kind will not be tolerated.

---

## Getting Started

1. Fork the repository and clone your fork.
2. Read the [README](README.md) for project overview and commands.
3. Read [DOLFIN.md](../../DOLFIN.md) to understand the language specification before touching the parser, compiler, or type system.

---

## How to Contribute

Contributions are welcome in these areas:

- **Language design**: proposing or refining Dolfin syntax and semantics
- **Compiler / parser**: the Rust core (`rust/`) and Python bindings (`python/`)
- **OWL/RDF output**: improving Turtle generation correctness or coverage
- **Tooling**: editor integrations, LSP, syntax highlighting (`editors/`)
- **Tests**: new test cases, especially for edge cases in the grammar
- **Documentation**: improving language reference, tutorials, or inline docs

---

## Submitting Changes

1. Create a branch off `main` with a descriptive name:
   ```
   git checkout -b fix/parser-indentation-edge-case
   git checkout -b feat/add-union-type-syntax
   ```

2. Make focused, atomic commits. Each commit should leave the project in a passing state.

3. Open a pull request against `main`. In the PR description include:
   - **What** changed and **why**
   - Any relevant issue numbers (`Closes #42`)
   - How to test or reproduce the scenario

4. A maintainer will review your PR. Be ready to iterate based on feedback.

---

## Coding Standards

- **Rust**: follow `rustfmt` defaults. Run `cargo fmt` and `cargo clippy` before pushing.
- **Python**: follow the existing style; the project uses `uv`: do not commit a `requirements.txt`.
- **Dolfin spec changes**: any syntax or semantic change must be reflected in [DOLFIN.md](../../DOLFIN.md) in the same PR.
- Write tests for new behavior. Bug fixes should include a regression test.

---

## Reporting Bugs

Open a GitHub Issue and include:

- A minimal `.dlf` snippet that reproduces the problem
- The expected output (Turtle / error message)
- The actual output
- Your OS, Rust toolchain version, and Python version

---

## Suggesting Features

Open a GitHub Issue with the `enhancement` label. Describe the use case first (what ontology pattern does this enable?) before proposing syntax. Language changes have a high bar: they must not break existing `.dlf` files.

---

We appreciate every contribution, large or small. Thank you for helping Dolfin grow.
