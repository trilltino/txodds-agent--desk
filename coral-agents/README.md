# coral-agents

World Cup Agent Desk keeps CoralOS-style agent identity separate from the desktop shell.

The Tauri app is the operator console. These manifests name the market actors it controls or observes:

| Agent | Role | Service |
| --- | --- | --- |
| `worldcup-buyer-agent` | buyer | Converts TxLINE triggers into WANTs and awards sellers. |
| `seller-worldcup-edge` | seller | Sells fixture-bound TxLINE fair-line reads. |
| `seller-risk-policy` | seller | Sells risk policy and no-action/observe/simulate guidance. |
| `seller-fan-card` | seller | Sells shareable fan-card output. |
| `verifier-agent` | verifier | Checks hash, fixture binding, proof shape, and policy gates. |
| `settlement-arbiter-agent` | settlement | Bridges verified runs to CoralOS settlement and Triton observation. |

This mirrors `solana_coralOS/coral-agents`: agent persona and runtime configuration live here, while shared runtime concerns live under `src-tauri/src/coral` and `src/domain/coral`.
