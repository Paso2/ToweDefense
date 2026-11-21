# Aethermaul TD — 3D Fantasy Mazing Tower Defense (Rust + Bevy 0.20)

Ein professionelles, datengetriebenes 3D-Fantasy-Tower-Defense-Spiel in **Rust (Edition 2024)** und **Bevy `0.20`**, inspiriert von klassischen Warcraft-3-Custom-Maps (*Wintermaul*, *Green TD*, *Element TD*).

## Direkt-Download für Windows (64-Bit)

Du musst nichts kompilieren — die fertige Windows-Version liegt direkt hier im Repository unter [`dist/`](dist/):

1. **[`dist/aethermaul_td.exe` direkt herunterladen (Single-File `.exe`, 58 MB)](https://github.com/Paso2/ToweDefense/raw/main/dist/aethermaul_td.exe)**  
   *(Einfach herunterladen und doppelklicken! Alle 3D-Modelle, PBR-Materialien, 44.1 kHz RAM-PCM-Soundeffekte, Schriftarten und `.ron`-Konfigurationen sind direkt in die `.exe` eingebettet.)*
2. **[`dist/AethermaulTD-v1.0.0-Windows-x64.zip` herunterladen (Portable `.zip` inkl. `config/*.ron`, 18 MB)](https://github.com/Paso2/ToweDefense/raw/main/dist/AethermaulTD-v1.0.0-Windows-x64.zip)**  
   *(Enthält zusätzlich den `config/`-Ordner neben der `.exe`, falls du Balancing-Werte, Wellen, Maps oder Fraktionen per Texteditor anpassen und im Spiel live mit `F5` neu laden möchtest.)*

> **Hinweis zu Windows SmartScreen:** Da die `.exe` frisch ohne kommerzielles Authenticode-Zertifikat gebaut wurde, zeigt Windows beim ersten Doppelklick evtl. *„Der PC wurde durch Windows geschützt“* an. Dort einfach auf **„Weitere Informationen“ → „Trotzdem ausführen“** klicken.

---

Vollständige Design- und Technik-Spezifikation: [`GAME_SPEC.md`](GAME_SPEC.md)

## Features
- **WC3-Live-Mazing & Multi-Stage-Flow-Field:** Freies Bauraster auf *Lordaeron-Bastion* (28×20) & *Wintermaul-Gletscher* (30×22) mit striktem *No-Corner-Cutting*, Echtzeit-Anti-Block-Validierung (Spawn + alle lebenden Bodengegner), `1.5 s` Reclaim-Cooldown, Anti-Juggle-Enrage und 3D-Pfadvorschau (Boden + Luft auf `y = 2.6`).
- **5 eigenständige Fraktionen & Spezialwährungen:**
  1. **Elementar-Zirkel** (*Elementar-Essenzen* — Feuer, Wasser, Blitz & Dual-Elemente: *Dampf*, *Plasma*, *Sturm*)
  2. **Zitadelle** (*Handwerkskerne* — Schnellfeuer, Mehrfachschuss, Belagerungs-Mörser, Flak & Gilden-Schatzkammer)
  3. **Drachenklan** (*Glutseelen* — Rot-, Bronze- & Sturmdrachen-Pfade + Goldhort-Schrein)
  4. **Frostwacht** (*Frostsplitter* — Slow, Freeze-Stacks, +80–110 % *Shatter*-Crit & Nordwind-Luftabwehr)
  5. **Nekropolis** (*Seelen* — Seuchen mit bis zu 95 % Anti-Heal, Armor-Shred bis `-45`, Banshee-Fluch & Seelen-Krypta)
- **WC3-Ökonomie & Leak-Loop:** Startgold, Kill-Bounty, passives Gold/s, Wellen-Bonus, 7 % Zinsen (Cap 250g), Early-Send-Bonus sowie WC3-Portal-Leak-Loop mit 45-Slot-Limit und 10 s Gnadenfrist.
- **Zero-Asset-Stand-Alone:** 100 % prozedurale PBR-3D-Modelle, Animationen, Partikel und 44.1 kHz RAM-PCM-SFX-Synthese; alle `.ron`-Konfigurationen (`config/**/*.ron`) und die UI-Schriftart sind direkt in die Binary eingebettet und optional über `./config/*.ron` modbar.

## Selbst kompilieren (Optional)

```bash
# Fast-Compile Development Build
cargo run --features dev_fast

# Linter & Unit-Tests
cargo clippy --features dev_fast --all-targets -- -D warnings
cargo test --features dev_fast

# Automatisierter 3-Stufen-Smoke-Test
cargo run --features dev_fast -- --smoke-test

# Optimierter Release-Build
cargo build --release
```