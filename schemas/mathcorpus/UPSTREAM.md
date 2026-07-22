# Pinned MathCorpus and MCIP schemas

MathOS vendors the minimum schema set needed to emit and verify its MathCorpus packet and
MCIP v1 projection without network access.

- Repository: `Mnehmos/mathcorpus`
- Commit: `a0d08c9ace0dcc70a8bc281dcf29c560242075d3`
- Tree: `62bc32fac877a82958ffcbe86402f8e793295f99`
- License: Apache-2.0; the exact upstream license is retained beside the packet schema.

The vendored files are byte-for-byte copies. Their SHA-256 identities are:

| File | SHA-256 |
| --- | --- |
| `packet.schema.json` | `6b6dfb3d558acbe53c9ca9e4d559f4e2677486e1a9d3b5c852a7cab6f7af532e` |
| `_defs.schema.json` | `d0201d4abdb106974de7b27f2a10909069e0aac1df490811bed3c15fec123137` |
| `bundle.schema.json` | `00eab00b02761d4e82574052d7c5547d7a1b70a49e434f87a5ff77f8c3e6fb49` |
| `packet_identity.schema.json` | `e40aab76c1682f8ee5be840c5eeae82f3e0da1572b2b322ed20f84ad74e69595` |
| `proof_variant.schema.json` | `497f1dce5e49e311a2af586c2bd035439724c13b85e37cb734909eccebbb5fdb` |
| `dependency_manifest.schema.json` | `4014bbb84b2e09bd838ca60365a8d44156342d3e44918236a37cc87c15eb8bbf` |

MathOS does not import MathCorpus as a runtime library. These pinned schemas are an offline
interchange contract; canonical MathOS release state remains the source of truth.
