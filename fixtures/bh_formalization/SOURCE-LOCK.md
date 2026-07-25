# Source Lock

## Pinned Environment Details

- **Lean 4 Toolchain**: `leanprover/lean4:v4.32.0-rc1`
- **Mathlib Commit**: `360da6fa66c1273b76b6b2d8c5666fd5ac2e3b56`
- **Plausible Commit**: `f3f26cc72646205ca167117487c008ee1dafe816`
- **LeanSearchClient Commit**: `c5d5b8fe6e5158def25cd28eb94e4141ad97c843`
- **Import-Graph Commit**: `41f407a8e85b0fdc00910633a8f14754139b63f4`
- **ProofWidgets4 Commit**: `e6518a674e62de322b8f79eebeda7bcae2a36bc3`
- **Aesop Commit**: `b5b9e2bb45ce91e4bc44eaa738c3a8910404ab82`
- **Quote4 (Qq) Commit**: `7a62bd13860cd39ac98da16ffc8c24d601353f69`
- **Batteries Commit**: `954dbc9873f3b4534dc9896604593406d0383520`
- **Lean4-Cli Commit**: `406ebb8c8e2f7e852a1b47764b42494022ce652c`

## Authoritative Paper Specifications

- **Paper**: arXiv:2607.12208v1, "The Benjamini-Hochberg Procedure Can Fail to Control the FDR for Correlated Two-Sided Gaussian Tests" by Edgar Dobriban.
- **Reproducibility Repository**: `dobriban/BH` on GitHub.

## Core Mathematical Model Parameters

For each $N \ge 1$, we define $m_N = 100N$.
The mutually independent standard normal random variables are:
- $Z \sim \mathcal{N}(0, 1)$ (the common factor)
- $\epsilon_i \sim \mathcal{N}(0, 1)$ for $1 \le i \le 96N$
- $\eta_j \sim \mathcal{N}(0, 1)$ for $1 \le j \le N$
- $\xi_k \sim \mathcal{N}(0, 1)$ for $1 \le k \le 3N$

Define:
- $X_{0, i} = \frac{3}{10} Z + \frac{\sqrt{91}}{10} \epsilon_i$ for $1 \le i \le 96N$ (True Nulls)
- $X_{1, j} = \frac{12}{5} - \frac{3}{10} Z + \frac{\sqrt{91}}{10} \eta_j$ for $1 \le j \le N$
- $X_{2, k} = \frac{22}{5} - \frac{18}{25} Z + \frac{\sqrt{301}}{25} \xi_k$ for $1 \le k \le 3N$

The two-sided p-value is:
- $P_i = 2 \cdot \Phi(|X_i|)$

The BH level is $\alpha = 0.01$ (or $1/100$).
The target theorem establishes that eventually, $\text{FDR}_N > 0.0104$.
