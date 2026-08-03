# Tecan Veya — Hardware Note (Evidence Insufficient)

## Purpose And Status

| Item | Value |
| --- | --- |
| Doc type | Hardware inventory for device-model design. Not protocol evidence. |
| Vendor | Tecan |
| Family | Veya (research) and Veya Dx (IVD) |
| Robot class | Deck liquid handler in the Fluent/EVO lineage, presented as a multiomics workflow platform |
| Evidence quality | **Low.** Only marketing pages found in this pass. No specification sheet, manual, or datasheet located. |

This page exists so that Veya is not silently omitted from the survey. Do not
design against it until a Tecan specification sheet is obtained.

## What Is Actually Documented

| Fact | Source quality |
| --- | --- |
| Announced/unveiled at SLAS (San Diego) | Marketing/news |
| Positioned as a multiomics platform with predeveloped scripts for research and clinical workflows | Vendor product page |
| Validated performance from 1 µL to 5 mL when used with Tecan consumables | Vendor product page |
| OneView display for system monitoring | Vendor product page |
| Integrated HEPA filter offered (cell biology configuration) | Vendor product page |
| Integrated cooling, heating and shaking modules on deck (proteomics configuration) | Vendor product page |
| Veya Dx is CE-IVD (EU 2017/746 Class A) and US Class 1 IVD, 510(k) exempt | Vendor/regulatory statement |

The 1 µL – 5 mL envelope matches the Tecan Liquid LiHa / FCA volume range, which
suggests Veya reuses the existing Tecan arm and channel hardware rather than
introducing a new pipetting mechanism. That is an inference, not evidence.

## What Is Not Known

| Area | Unknown |
| --- | --- |
| Arms | Number, types, and whether FCA / MCA / RGA nomenclature carries over |
| Channels | Channel counts, independent Z/Y behaviour, tip families |
| Deck | Size variants, grid/carrier addressing, position counts |
| Sensors | LLD types, pressure monitoring, deck vision |
| Modules | Which on-deck thermal/shaking modules, and their control interfaces |
| Physical | Dimensions, weight, power |
| Control | Whether FluentControl, a new stack, or a SiLA2 interface drives it |

## Device-Model Implications

Provisionally treat Veya as a Fluent-shaped platform: see
[`tecan-fluent.md`](tecan-fluent.md) for the arm/channel/head/gripper
decomposition, and note the two Veya-specific additions worth planning for:

| Feature | Model consequence |
| --- | --- |
| On-deck cooling/heating/shaking modules as a standard configuration | Reinforces the need for `module.temperature`, `module.shaker`, `module.heater_shaker` as deck-resident child devices |
| Enclosure services (HEPA) | An enclosure/environment device kind may be needed (airflow, filter state) that is unrelated to liquid handling |

## Evidence

| Evidence | Link |
| --- | --- |
| Veya product page | <https://lifesciences.tecan.com/products/veya-effortless-automation> |
| Tecan liquid handling and automation portfolio page | <https://lifesciences.tecan.com/products/liquid_handling_and_automation> |

## Next Step

Request or locate the Veya specification sheet (the Fluent equivalent is Tecan
doc 398328) before adding any Veya-specific structure to the device model.
