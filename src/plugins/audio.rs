use std::collections::HashMap;
use std::path::PathBuf;

use bevy::prelude::*;

use crate::types::*;

/// Minimum cooldown in seconds between identical SFX cues to avoid clipping during heavy combat.
pub const SFX_COOLDOWN_SECS: f32 = 0.050;

/// Sample rate for all synthesized RAM-PCM WAV buffers.
pub const SAMPLE_RATE: u32 = 44_100;

/// Complete list of all SFX variants for startup pre-synthesis and testing.
pub const ALL_SFX_KINDS: [SfxEventKind; 17] = [
    SfxEventKind::ArrowShot,
    SfxEventKind::FireCast,
    SfxEventKind::Explosion,
    SfxEventKind::IceHit,
    SfxEventKind::Freeze,
    SfxEventKind::LightningZap,
    SfxEventKind::PoisonCast,
    SfxEventKind::BuildTower,
    SfxEventKind::UpgradeTower,
    SfxEventKind::SellTower,
    SfxEventKind::GoldCoin,
    SfxEventKind::LimitWarning,
    SfxEventKind::WaveStart,
    SfxEventKind::PortalWarp,
    SfxEventKind::VictoryFanfare,
    SfxEventKind::DefeatHorn,
    SfxEventKind::UiClick,
];

/// Returns the canonical filename stem used for optional `./audio/<name>.ogg` overrides.
pub fn sfx_override_stem(kind: SfxEventKind) -> &'static str {
    match kind {
        SfxEventKind::ArrowShot => "arrow_shot",
        SfxEventKind::FireCast => "fire_cast",
        SfxEventKind::Explosion => "explosion",
        SfxEventKind::IceHit => "ice_hit",
        SfxEventKind::Freeze => "freeze",
        SfxEventKind::LightningZap => "lightning_zap",
        SfxEventKind::PoisonCast => "poison_cast",
        SfxEventKind::BuildTower => "build_tower",
        SfxEventKind::UpgradeTower => "upgrade_tower",
        SfxEventKind::SellTower => "sell_tower",
        SfxEventKind::GoldCoin => "gold_coin",
        SfxEventKind::LimitWarning => "limit_warning",
        SfxEventKind::WaveStart => "wave_start",
        SfxEventKind::PortalWarp => "portal_warp",
        SfxEventKind::VictoryFanfare => "victory_fanfare",
        SfxEventKind::DefeatHorn => "defeat_horn",
        SfxEventKind::UiClick => "ui_click",
    }
}

/// Resource holding all synthesized RAM-PCM WAV buffers, optional `.ogg` overrides,
/// per-event polyphony cooldowns, and diagnostic counters.
#[derive(Resource, Debug)]
pub struct ProceduralAudioBank {
    pub wav_buffers: HashMap<SfxEventKind, Vec<u8>>,
    pub ogg_overrides: HashMap<SfxEventKind, Vec<u8>>,
    #[cfg(any(not(target_os = "linux"), feature = "linux_audio"))]
    pub audio_handles: HashMap<SfxEventKind, Handle<AudioSource>>,
    pub last_played_secs: HashMap<SfxEventKind, f64>,
    pub played_count: u64,
    pub muted_skips: u64,
    pub cooldown_skips: u64,
}

impl Default for ProceduralAudioBank {
    fn default() -> Self {
        Self::new_synthesized()
    }
}

impl ProceduralAudioBank {
    /// Builds the audio bank synchronously with synthesized WAV buffers for every `SfxEventKind`,
    /// checking `./audio/<name>.ogg` (relative to cwd or executable directory) for optional overrides.
    pub fn new_synthesized() -> Self {
        let mut wav_buffers = HashMap::with_capacity(ALL_SFX_KINDS.len());
        let mut ogg_overrides = HashMap::new();

        let exe_audio_dir: Option<PathBuf> = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("audio")));

        for kind in ALL_SFX_KINDS {
            let wav = synthesize_sfx_wav(kind);
            wav_buffers.insert(kind, wav);

            let file_name = format!("{}.ogg", sfx_override_stem(kind));
            let local_path = PathBuf::from("audio").join(&file_name);
            if let Ok(bytes) = std::fs::read(&local_path)
                && !bytes.is_empty()
            {
                ogg_overrides.insert(kind, bytes);
                continue;
            }
            if let Some(ref dir) = exe_audio_dir {
                let exe_path = dir.join(&file_name);
                if let Ok(bytes) = std::fs::read(exe_path)
                    && !bytes.is_empty()
                {
                    ogg_overrides.insert(kind, bytes);
                }
            }
        }

        Self {
            wav_buffers,
            ogg_overrides,
            #[cfg(any(not(target_os = "linux"), feature = "linux_audio"))]
            audio_handles: HashMap::new(),
            last_played_secs: HashMap::new(),
            played_count: 0,
            muted_skips: 0,
            cooldown_skips: 0,
        }
    }

    /// Returns raw audio bytes for a sound event (external `.ogg` override if present, else RAM-PCM WAV).
    pub fn bytes_for(&self, kind: SfxEventKind) -> Option<&[u8]> {
        self.ogg_overrides
            .get(&kind)
            .or_else(|| self.wav_buffers.get(&kind))
            .map(Vec::as_slice)
    }

    /// Evaluates mute, volume, and 50 ms per-kind cooldown rules.
    /// Returns `Some(final_linear_volume)` if playback should occur, or `None` if skipped.
    pub fn try_trigger(
        &mut self,
        kind: SfxEventKind,
        volume_scale: f32,
        now_secs: f64,
        settings: &GameSettings,
    ) -> Option<f32> {
        if settings.muted {
            self.muted_skips += 1;
            return None;
        }

        let final_vol =
            (settings.master_volume * settings.sfx_volume * volume_scale).clamp(0.0, 1.5);
        if final_vol <= 0.001 {
            self.muted_skips += 1;
            return None;
        }

        if let Some(&last_t) = self.last_played_secs.get(&kind)
            && (now_secs - last_t) < f64::from(SFX_COOLDOWN_SECS)
        {
            self.cooldown_skips += 1;
            return None;
        }

        self.last_played_secs.insert(kind, now_secs);
        self.played_count += 1;
        Some(final_vol)
    }
}

pub struct AudioPlugin;

impl Plugin for AudioPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ProceduralAudioBank>()
            .add_systems(Startup, init_audio_bank_assets)
            .add_systems(Update, process_sfx_messages.in_set(GameSet::VfxUiSet));
    }
}

#[cfg(any(not(target_os = "linux"), feature = "linux_audio"))]
fn init_audio_bank_assets(
    mut bank: ResMut<ProceduralAudioBank>,
    mut audio_assets: ResMut<Assets<AudioSource>>,
) {
    let mut handles = HashMap::with_capacity(ALL_SFX_KINDS.len());
    for kind in ALL_SFX_KINDS {
        if let Some(bytes) = bank.bytes_for(kind) {
            let source = AudioSource {
                bytes: bytes.to_vec().into(),
            };
            let handle = audio_assets.add(source);
            handles.insert(kind, handle);
        }
    }
    bank.audio_handles = handles;
    info!(
        "ProceduralAudioBank initialized ({} RAM-PCM SFX, {} external .ogg overrides)",
        bank.wav_buffers.len(),
        bank.ogg_overrides.len()
    );
}

#[cfg(not(any(not(target_os = "linux"), feature = "linux_audio")))]
fn init_audio_bank_assets(bank: Res<ProceduralAudioBank>) {
    info!(
        "ProceduralAudioBank synthesized {} RAM-PCM SFX buffers (playback disabled on Linux without `linux_audio` feature)",
        bank.wav_buffers.len()
    );
}

#[allow(unused_variables, unused_mut)]
fn process_sfx_messages(
    mut commands: Commands,
    mut reader: MessageReader<PlaySfxMsg>,
    mut bank: ResMut<ProceduralAudioBank>,
    settings: Res<GameSettings>,
    time: Res<Time>,
) {
    let now_secs = time.elapsed_secs_f64();

    for msg in reader.read() {
        let Some(final_vol) = bank.try_trigger(msg.kind, msg.volume_scale, now_secs, &settings)
        else {
            continue;
        };

        let pitch = 0.94 + rand::random::<f32>() * 0.12;

        #[cfg(any(not(target_os = "linux"), feature = "linux_audio"))]
        if let Some(handle) = bank.audio_handles.get(&msg.kind).cloned() {
            commands.spawn((
                AudioPlayer::new(handle),
                PlaybackSettings::DESPAWN
                    .with_volume(bevy::audio::Volume::Linear(final_vol))
                    .with_speed(pitch),
            ));
        }

        #[cfg(not(any(not(target_os = "linux"), feature = "linux_audio")))]
        let _ = (final_vol, pitch, &mut commands);
    }
}

// ============================================================================
// Pure Rust RAM-PCM RIFF/WAVE 16-bit Mono 44.1 kHz Synthesizer
// ============================================================================

/// Synthesizes a complete, valid RIFF/WAVE 16-bit mono 44.1 kHz PCM buffer for `kind`.
pub fn synthesize_sfx_wav(kind: SfxEventKind) -> Vec<u8> {
    let samples = match kind {
        SfxEventKind::ArrowShot => synth_arrow_shot(),
        SfxEventKind::FireCast => synth_fire_cast(),
        SfxEventKind::Explosion => synth_explosion(),
        SfxEventKind::IceHit => synth_ice_hit(),
        SfxEventKind::Freeze => synth_freeze(),
        SfxEventKind::LightningZap => synth_lightning_zap(),
        SfxEventKind::PoisonCast => synth_poison_cast(),
        SfxEventKind::BuildTower => synth_build_tower(),
        SfxEventKind::UpgradeTower => synth_upgrade_tower(),
        SfxEventKind::SellTower => synth_sell_tower(),
        SfxEventKind::GoldCoin => synth_gold_coin(),
        SfxEventKind::LimitWarning => synth_limit_warning(),
        SfxEventKind::WaveStart => synth_wave_start(),
        SfxEventKind::PortalWarp => synth_portal_warp(),
        SfxEventKind::VictoryFanfare => synth_victory_fanfare(),
        SfxEventKind::DefeatHorn => synth_defeat_horn(),
        SfxEventKind::UiClick => synth_ui_click(),
    };
    encode_wav_pcm16_mono(SAMPLE_RATE, &samples)
}

/// Encodes normalized `f32` mono samples (`-1.0..=1.0`) into a standard 44-byte RIFF/WAVE PCM16 buffer.
pub fn encode_wav_pcm16_mono(sample_rate: u32, samples: &[f32]) -> Vec<u8> {
    let num_channels: u16 = 1;
    let bits_per_sample: u16 = 16;
    let byte_rate: u32 = sample_rate * u32::from(num_channels) * u32::from(bits_per_sample / 8);
    let block_align: u16 = num_channels * (bits_per_sample / 8);
    let data_bytes: u32 = (samples.len() * 2) as u32;
    let riff_chunk_size: u32 = 36 + data_bytes;

    let mut buf = Vec::with_capacity(44 + data_bytes as usize);
    buf.extend_from_slice(b"RIFF");
    buf.extend_from_slice(&riff_chunk_size.to_le_bytes());
    buf.extend_from_slice(b"WAVE");
    buf.extend_from_slice(b"fmt ");
    buf.extend_from_slice(&16_u32.to_le_bytes()); // PCM fmt chunk size
    buf.extend_from_slice(&1_u16.to_le_bytes()); // AudioFormat = 1 (PCM)
    buf.extend_from_slice(&num_channels.to_le_bytes());
    buf.extend_from_slice(&sample_rate.to_le_bytes());
    buf.extend_from_slice(&byte_rate.to_le_bytes());
    buf.extend_from_slice(&block_align.to_le_bytes());
    buf.extend_from_slice(&bits_per_sample.to_le_bytes());
    buf.extend_from_slice(b"data");
    buf.extend_from_slice(&data_bytes.to_le_bytes());

    for &s in samples {
        let clamped = s.clamp(-1.0, 1.0);
        let pcm = (clamped * i16::MAX as f32).round() as i16;
        buf.extend_from_slice(&pcm.to_le_bytes());
    }
    buf
}

// --- DSP Primitives ---

struct DeterministicNoise {
    state: u32,
    lp_prev: f32,
    hp_prev: f32,
    hp_in_prev: f32,
    brown_acc: f32,
}

impl DeterministicNoise {
    fn new(seed: u32) -> Self {
        Self {
            state: seed.max(1),
            lp_prev: 0.0,
            hp_prev: 0.0,
            hp_in_prev: 0.0,
            brown_acc: 0.0,
        }
    }

    fn next_white(&mut self) -> f32 {
        // Xorshift32
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.state = x;
        (x as f32 / u32::MAX as f32) * 2.0 - 1.0
    }

    fn next_lowpass(&mut self, alpha: f32) -> f32 {
        let w = self.next_white();
        self.lp_prev += alpha * (w - self.lp_prev);
        self.lp_prev
    }

    fn next_bandpass(&mut self, lp_alpha: f32, hp_alpha: f32) -> f32 {
        let lp = self.next_lowpass(lp_alpha);
        let hp = hp_alpha * (self.hp_prev + lp - self.hp_in_prev);
        self.hp_in_prev = lp;
        self.hp_prev = hp;
        hp
    }

    fn next_brown(&mut self, leak: f32) -> f32 {
        let w = self.next_white();
        self.brown_acc = (self.brown_acc * leak + w * 0.12).clamp(-1.0, 1.0);
        self.brown_acc
    }
}

#[inline]
fn osc_sine(phase: f32) -> f32 {
    (phase * std::f32::consts::TAU).sin()
}

#[inline]
fn osc_triangle(phase: f32) -> f32 {
    let p = phase.fract();
    if p < 0.25 {
        p * 4.0
    } else if p < 0.75 {
        2.0 - p * 4.0
    } else {
        p * 4.0 - 4.0
    }
}

#[inline]
fn osc_saw(phase: f32) -> f32 {
    phase.fract() * 2.0 - 1.0
}

#[inline]
fn osc_square(phase: f32) -> f32 {
    if phase.fract() < 0.5 { 1.0 } else { -1.0 }
}

/// Simple ADSR amplitude envelope in `[0.0, 1.0]`.
#[inline]
fn adsr(t: f32, dur: f32, attack: f32, decay: f32, sustain: f32, release: f32) -> f32 {
    if t < 0.0 || t >= dur {
        return 0.0;
    }
    let rel_start = (dur - release).max(attack + decay);
    if t < attack {
        (t / attack.max(1e-4)).clamp(0.0, 1.0)
    } else if t < attack + decay {
        let u = (t - attack) / decay.max(1e-4);
        1.0 + (sustain - 1.0) * u.clamp(0.0, 1.0)
    } else if t < rel_start {
        sustain
    } else {
        let u = (t - rel_start) / (dur - rel_start).max(1e-4);
        sustain * (1.0 - u.clamp(0.0, 1.0))
    }
}

fn sample_count(duration_secs: f32) -> usize {
    (duration_secs * SAMPLE_RATE as f32).round().max(1.0) as usize
}

// --- Individual Procedural Sound Recipes ---

fn synth_arrow_shot() -> Vec<f32> {
    let dur = 0.085;
    let n = sample_count(dur);
    let mut out = Vec::with_capacity(n);
    let mut noise = DeterministicNoise::new(0xA110_0001);
    let mut phase = 0.0_f32;
    let dt = 1.0 / SAMPLE_RATE as f32;

    for i in 0..n {
        let t = i as f32 * dt;
        let u = t / dur;
        let freq = 920.0 - 620.0 * u;
        phase += freq * dt;
        let tri = osc_triangle(phase);
        let bp_noise = if t < 0.042 {
            noise.next_bandpass(0.32, 0.85) * (1.0 - t / 0.042)
        } else {
            0.0
        };
        let env = adsr(t, dur, 0.003, 0.025, 0.25, 0.04);
        out.push((tri * 0.45 + bp_noise * 0.75) * env * 0.65);
    }
    out
}

fn synth_fire_cast() -> Vec<f32> {
    let dur = 0.21;
    let n = sample_count(dur);
    let mut out = Vec::with_capacity(n);
    let mut noise = DeterministicNoise::new(0xF1AE_0002);
    let mut phase = 0.0_f32;
    let dt = 1.0 / SAMPLE_RATE as f32;

    for i in 0..n {
        let t = i as f32 * dt;
        let u = t / dur;
        let freq = 140.0 - 95.0 * u;
        phase += freq * dt;
        let sub = osc_sine(phase);
        let roar = noise.next_brown(0.96) * 0.7 + noise.next_lowpass(0.14) * 0.5;
        let env = adsr(t, dur, 0.015, 0.06, 0.45, 0.11);
        out.push((sub * 0.45 + roar * 0.75) * env * 0.72);
    }
    out
}

fn synth_explosion() -> Vec<f32> {
    let dur = 0.34;
    let n = sample_count(dur);
    let mut out = Vec::with_capacity(n);
    let mut noise = DeterministicNoise::new(0xE091_0003);
    let mut phase = 0.0_f32;
    let dt = 1.0 / SAMPLE_RATE as f32;

    for i in 0..n {
        let t = i as f32 * dt;
        let u = t / dur;
        let freq = 115.0 * (1.0 - u * 0.72);
        phase += freq * dt;
        let sub = osc_sine(phase);
        let crunch = noise.next_brown(0.97) * 0.85 + noise.next_lowpass(0.22) * 0.55;
        let env = adsr(t, dur, 0.004, 0.09, 0.38, 0.20);
        out.push((sub * 0.62 + crunch * 0.65).tanh() * env * 0.82);
    }
    out
}

fn synth_ice_hit() -> Vec<f32> {
    let dur = 0.16;
    let n = sample_count(dur);
    let mut out = Vec::with_capacity(n);
    let mut noise = DeterministicNoise::new(0x1CE0_0004);
    let dt = 1.0 / SAMPLE_RATE as f32;

    for i in 0..n {
        let t = i as f32 * dt;
        let carrier = (t * 1820.0 * std::f32::consts::TAU
            + 1.6 * (t * 2740.0 * std::f32::consts::TAU).sin())
        .sin();
        let chime = osc_sine(t * 2760.0) * 0.4;
        let shimmer = noise.next_bandpass(0.65, 0.92) * 0.25 * (-t * 26.0).exp();
        let env = adsr(t, dur, 0.002, 0.05, 0.18, 0.09);
        out.push((carrier * 0.55 + chime + shimmer) * env * 0.55);
    }
    out
}

fn synth_freeze() -> Vec<f32> {
    let dur = 0.28;
    let n = sample_count(dur);
    let mut out = Vec::with_capacity(n);
    let mut noise = DeterministicNoise::new(0xF2EE_0005);
    let dt = 1.0 / SAMPLE_RATE as f32;

    for i in 0..n {
        let t = i as f32 * dt;
        let mod_sig = (t * 3150.0 * std::f32::consts::TAU).sin() * 2.2 * (-t * 10.0).exp();
        let fm1 = (t * 1560.0 * std::f32::consts::TAU + mod_sig).sin();
        let fm2 = osc_sine(t * 2340.0) * 0.45;
        let crackle = noise.next_bandpass(0.7, 0.94) * 0.35 * (-t * 14.0).exp();
        let env = adsr(t, dur, 0.005, 0.09, 0.28, 0.16);
        out.push((fm1 * 0.5 + fm2 + crackle) * env * 0.60);
    }
    out
}

fn synth_lightning_zap() -> Vec<f32> {
    let dur = 0.11;
    let n = sample_count(dur);
    let mut out = Vec::with_capacity(n);
    let mut noise = DeterministicNoise::new(0xB172_0006);
    let mut phase = 0.0_f32;
    let dt = 1.0 / SAMPLE_RATE as f32;

    for i in 0..n {
        let t = i as f32 * dt;
        let jitter = noise.next_white() * 900.0;
        let freq = (1350.0 - 700.0 * (t / dur) + jitter).clamp(350.0, 2400.0);
        phase += freq * dt;
        let sq = osc_square(phase);
        let saw = osc_saw(phase * 0.5);
        let clipped = (sq * 0.65 + saw * 0.45).clamp(-0.75, 0.75);
        let env = adsr(t, dur, 0.002, 0.03, 0.35, 0.06);
        out.push(clipped * env * 0.55);
    }
    out
}

fn synth_poison_cast() -> Vec<f32> {
    let dur = 0.22;
    let n = sample_count(dur);
    let mut out = Vec::with_capacity(n);
    let mut noise = DeterministicNoise::new(0x9015_0007);
    let mut phase = 0.0_f32;
    let dt = 1.0 / SAMPLE_RATE as f32;

    for i in 0..n {
        let t = i as f32 * dt;
        let wobble = (t * 28.0 * std::f32::consts::TAU).sin() * 95.0;
        let freq = 220.0 + 180.0 * (t / dur) + wobble;
        phase += freq * dt;
        let bubble = osc_sine(phase) * 0.55;
        let hiss = noise.next_bandpass(0.35, 0.85) * 0.45;
        let env = adsr(t, dur, 0.012, 0.06, 0.35, 0.11);
        out.push((bubble + hiss) * env * 0.60);
    }
    out
}

fn synth_build_tower() -> Vec<f32> {
    let dur = 0.12;
    let n = sample_count(dur);
    let mut out = Vec::with_capacity(n);
    let mut noise = DeterministicNoise::new(0xB01D_0008);
    let mut phase = 0.0_f32;
    let dt = 1.0 / SAMPLE_RATE as f32;

    for i in 0..n {
        let t = i as f32 * dt;
        let freq = 155.0 - 100.0 * (t / dur);
        phase += freq * dt;
        let thud = osc_sine(phase) * 0.75;
        let click = if t < 0.018 {
            noise.next_lowpass(0.45) * (1.0 - t / 0.018) * 0.6
        } else {
            0.0
        };
        let env = adsr(t, dur, 0.002, 0.04, 0.20, 0.06);
        out.push((thud + click) * env * 0.78);
    }
    out
}

fn synth_upgrade_tower() -> Vec<f32> {
    let dur = 0.25;
    let n = sample_count(dur);
    let mut out = Vec::with_capacity(n);
    let dt = 1.0 / SAMPLE_RATE as f32;
    // C5 - E5 - G5 fantasy major arpeggio
    let notes = [(0.0, 523.25), (0.065, 659.25), (0.13, 783.99)];

    for i in 0..n {
        let t = i as f32 * dt;
        let mut sum = 0.0_f32;
        for &(start, freq) in &notes {
            if t >= start {
                let nt = t - start;
                let n_env = adsr(nt, dur - start, 0.004, 0.04, 0.45, 0.08);
                let tone = osc_triangle(nt * freq) * 0.65 + osc_sine(nt * freq * 2.0) * 0.25;
                sum += tone * n_env;
            }
        }
        out.push((sum * 0.42).clamp(-0.95, 0.95));
    }
    out
}

fn synth_sell_tower() -> Vec<f32> {
    let dur = 0.17;
    let n = sample_count(dur);
    let mut out = Vec::with_capacity(n);
    let dt = 1.0 / SAMPLE_RATE as f32;
    let notes = [(0.0, 660.0), (0.06, 440.0)];

    for i in 0..n {
        let t = i as f32 * dt;
        let mut sum = 0.0_f32;
        for &(start, freq) in &notes {
            if t >= start {
                let nt = t - start;
                let env = adsr(nt, 0.10, 0.003, 0.03, 0.3, 0.05);
                sum += (osc_triangle(nt * freq) * 0.6 + osc_sine(nt * freq * 2.0) * 0.2) * env;
            }
        }
        out.push(sum * 0.55);
    }
    out
}

fn synth_gold_coin() -> Vec<f32> {
    let dur = 0.14;
    let n = sample_count(dur);
    let mut out = Vec::with_capacity(n);
    let dt = 1.0 / SAMPLE_RATE as f32;

    for i in 0..n {
        let t = i as f32 * dt;
        let freq = if t < 0.045 { 1318.5 } else { 1760.0 };
        let env = adsr(t, dur, 0.002, 0.04, 0.35, 0.07);
        let sig = osc_sine(t * freq) * 0.75 + osc_sine(t * freq * 2.0) * 0.2;
        out.push(sig * env * 0.55);
    }
    out
}

fn synth_limit_warning() -> Vec<f32> {
    let dur = 0.55;
    let n = sample_count(dur);
    let mut out = Vec::with_capacity(n);
    let dt = 1.0 / SAMPLE_RATE as f32;
    let mut lp = 0.0_f32;

    for i in 0..n {
        let t = i as f32 * dt;
        // Deep fantasy war-horn: A2 (110 Hz) + E3 (165 Hz) damped sawtooth fifth
        let raw = osc_saw(t * 110.0) * 0.6 + osc_saw(t * 165.0) * 0.4 + osc_sine(t * 55.0) * 0.35;
        lp += 0.08 * (raw - lp);
        let env = adsr(t, dur, 0.04, 0.12, 0.65, 0.22);
        out.push(lp * env * 0.68);
    }
    out
}

fn synth_wave_start() -> Vec<f32> {
    let dur = 0.36;
    let n = sample_count(dur);
    let mut out = Vec::with_capacity(n);
    let dt = 1.0 / SAMPLE_RATE as f32;
    let mut lp = 0.0_f32;

    for i in 0..n {
        let t = i as f32 * dt;
        let u = t / dur;
        let base_freq = 146.83 + 73.42 * (u * 1.5).min(1.0); // D3 -> A3 horn swell
        let raw = osc_saw(t * base_freq) * 0.55 + osc_triangle(t * base_freq * 1.5) * 0.45;
        lp += 0.12 * (raw - lp);
        let env = adsr(t, dur, 0.03, 0.08, 0.60, 0.16);
        out.push(lp * env * 0.65);
    }
    out
}

fn synth_portal_warp() -> Vec<f32> {
    let dur = 0.26;
    let n = sample_count(dur);
    let mut out = Vec::with_capacity(n);
    let dt = 1.0 / SAMPLE_RATE as f32;
    let mut phase = 0.0_f32;

    for i in 0..n {
        let t = i as f32 * dt;
        let u = t / dur;
        let freq = 260.0 + 480.0 * (u * std::f32::consts::PI).sin();
        phase += freq * dt;
        let sig = osc_sine(phase) * 0.65 + osc_triangle(phase * 2.01) * 0.3;
        let env = adsr(t, dur, 0.01, 0.07, 0.40, 0.13);
        out.push(sig * env * 0.55);
    }
    out
}

fn synth_victory_fanfare() -> Vec<f32> {
    let dur = 0.65;
    let n = sample_count(dur);
    let mut out = Vec::with_capacity(n);
    let dt = 1.0 / SAMPLE_RATE as f32;
    // C5 - E5 - G5 - C6 triumphant chord fanfare
    let notes = [
        (0.0, 523.25),
        (0.11, 659.25),
        (0.22, 783.99),
        (0.33, 1046.50),
    ];

    for i in 0..n {
        let t = i as f32 * dt;
        let mut sum = 0.0_f32;
        for &(start, freq) in &notes {
            if t >= start {
                let nt = t - start;
                let env = adsr(nt, dur - start, 0.008, 0.08, 0.55, 0.18);
                let voice = osc_triangle(nt * freq) * 0.55
                    + osc_saw(nt * freq) * 0.20
                    + osc_sine(nt * freq * 0.5) * 0.25;
                sum += voice * env;
            }
        }
        out.push((sum * 0.35).clamp(-0.95, 0.95));
    }
    out
}

fn synth_defeat_horn() -> Vec<f32> {
    let dur = 0.62;
    let n = sample_count(dur);
    let mut out = Vec::with_capacity(n);
    let dt = 1.0 / SAMPLE_RATE as f32;
    let mut lp = 0.0_f32;

    for i in 0..n {
        let t = i as f32 * dt;
        let u = t / dur;
        // Descending minor horn 165 -> 98 Hz
        let freq = 165.0 - 67.0 * u;
        let raw = osc_saw(t * freq) * 0.6 + osc_saw(t * freq * 1.189) * 0.35; // minor third dissonance
        lp += 0.07 * (raw - lp);
        let env = adsr(t, dur, 0.04, 0.12, 0.60, 0.24);
        out.push(lp * env * 0.68);
    }
    out
}

fn synth_ui_click() -> Vec<f32> {
    let dur = 0.032;
    let n = sample_count(dur);
    let mut out = Vec::with_capacity(n);
    let dt = 1.0 / SAMPLE_RATE as f32;

    for i in 0..n {
        let t = i as f32 * dt;
        let env = adsr(t, dur, 0.001, 0.012, 0.1, 0.015);
        let sig = osc_sine(t * 1120.0) * 0.65 + osc_triangle(t * 1680.0) * 0.35;
        out.push(sig * env * 0.45);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_sfx_variants_produce_valid_riff_wave_headers_and_pcm() {
        for kind in ALL_SFX_KINDS {
            let wav = synthesize_sfx_wav(kind);
            assert!(wav.len() > 44, "WAV too short for {:?}", kind);
            assert_eq!(&wav[0..4], b"RIFF", "Missing RIFF header for {:?}", kind);
            assert_eq!(&wav[8..12], b"WAVE", "Missing WAVE marker for {:?}", kind);
            assert_eq!(&wav[12..16], b"fmt ", "Missing fmt chunk for {:?}", kind);
            assert_eq!(&wav[36..40], b"data", "Missing data chunk for {:?}", kind);

            let data_size = u32::from_le_bytes([wav[40], wav[41], wav[42], wav[43]]) as usize;
            assert!(data_size > 0, "Zero PCM payload for {:?}", kind);
            assert_eq!(data_size % 2, 0, "Invalid 16-bit sample byte alignment");
            assert_eq!(wav.len(), 44 + data_size);

            let sample_count = data_size / 2;
            assert!(sample_count >= 500, "Too few samples for {:?}", kind);
        }
    }

    #[test]
    fn cooldown_and_mute_logic_respects_polyphony_and_settings() {
        let mut bank = ProceduralAudioBank::new_synthesized();
        let mut settings = GameSettings::default();

        // First trigger succeeds
        let vol1 = bank.try_trigger(SfxEventKind::ArrowShot, 1.0, 1.000, &settings);
        assert!(vol1.is_some());
        assert_eq!(bank.played_count, 1);

        // Within 50ms cooldown -> skipped
        let vol2 = bank.try_trigger(SfxEventKind::ArrowShot, 1.0, 1.025, &settings);
        assert!(vol2.is_none());
        assert_eq!(bank.cooldown_skips, 1);

        // Different sound kind within 50ms -> allowed
        let vol_other = bank.try_trigger(SfxEventKind::GoldCoin, 1.0, 1.025, &settings);
        assert!(vol_other.is_some());
        assert_eq!(bank.played_count, 2);

        // After 50ms cooldown -> ArrowShot allowed again
        let vol3 = bank.try_trigger(SfxEventKind::ArrowShot, 1.0, 1.060, &settings);
        assert!(vol3.is_some());
        assert_eq!(bank.played_count, 3);

        // Muted -> skipped
        settings.muted = true;
        let vol_muted = bank.try_trigger(SfxEventKind::Explosion, 1.0, 2.000, &settings);
        assert!(vol_muted.is_none());
        assert_eq!(bank.muted_skips, 1);
    }
}
