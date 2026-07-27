# Third-party notices

LUMORA itself is licensed under the MIT License (see [`LICENSE`](../LICENSE)).

Optional on-device models are **not** bundled with the app by default. When you
download them from Settings → AI, each file’s licence is shown in the model
library. Typical terms:

| Capability | Example backend | Typical licence |
| --- | --- | --- |
| Semantic search | CLIP ViT-B/32 (OpenAI / community ONNX) | MIT |
| OCR | RapidOCR / PaddleOCR PP-OCR | Apache-2.0 |
| Auto-tags | MobileNetV4 (timm / ImageNet) | Apache-2.0 |
| Faces | InsightFace buffalo_l / buffalo_s | InsightFace **non-commercial research** terms |

InsightFace face models are opt-in. Do **not** use them in commercial products
unless you obtain a licence that permits it. Prefer switching to another
backend or leaving face recognition disabled if your use case is commercial.

Bundled offline Places data uses GeoNames (CC-BY 4.0) via the
`reverse_geocoder` crate — attribution applies when redistributing that data.

The Places map land silhouette is derived from [Natural Earth](https://www.naturalearthdata.com/)
110m land (public domain) as local SVG geometry — no map tile servers are contacted.

Always verify the licence text that ships with a specific model download before
redistributing model weights.
