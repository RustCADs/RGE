//! Per-frame mixer update.
//!
//! [`audio_schedule_step`] is the single function the host's main schedule
//! calls each tick. It walks the world's audio entities, reconciles each
//! [`AudioSource`](crate::AudioSource) against its tracked
//! [`SourceState`](crate::source::SourceState), and pipes Transform pose
//! updates into the registered listeners and emitters.
//!
//! ## Why a free function
//!
//! W12 lands before the kernel ECS substrate does — there is no
//! `World`/`SystemSet` to hang this on yet. We accept inputs as plain slices
//! to keep the surface small and easy to drive from tests. When the ECS
//! kernel arrives in W02 the function will be wrapped in an actual system,
//! but the body stays the same.

use kira::{Decibels, PlaybackRate, Tween};

use crate::components::{AudioSource, Entity, Transform};
use crate::manager::{AudioManager, ManagerError};
use crate::source::PlaybackState;

/// Convert a linear amplitude multiplier (`0.0..=N`, where `1.0` is unity gain
/// in our component model) to Kira 0.12's [`Decibels`] volume parameter.
///
/// Returns [`Decibels::SILENCE`] for non-positive amplitudes (below the audible
/// threshold) and otherwise applies `20 · log10(amp)`.
fn amplitude_to_decibels(amp: f32) -> Decibels {
    if amp <= 0.0 {
        Decibels::SILENCE
    } else {
        Decibels(20.0 * amp.log10())
    }
}

/// One audio entity's input data for [`audio_schedule_step`]. Lives separately
/// from [`AudioSource`] because the schedule step needs the entity ID + the
/// matching [`Transform`] together.
#[derive(Debug, Clone)]
pub struct AudioSchedule<'a> {
    /// Entity owning the [`AudioSource`].
    pub entity: Entity,
    /// World pose for the entity — drives Kira emitter position.
    pub transform: &'a Transform,
    /// Component data — playback intent, volume, pitch, looped, falloff.
    pub source: &'a AudioSource,
}

/// Run one mixer update tick.
///
/// `sources` is every audio-emitting entity active this frame. `listener`
/// is the (entity, transform) pair for the active audio listener (typically
/// the active camera). When `listener` is `None` the schedule still
/// reconciles emitter pose / state so that calling code can defer listener
/// setup without losing playback.
///
/// # Errors
///
/// Surfaces [`ManagerError::UnknownClip`] if an `AudioSource` references a
/// clip key not pre-loaded with [`AudioManager::register_clip`]. All other
/// failures (resource-pool exhaustion, dispatch errors) are logged and
/// skipped — one bad source must not stall the whole audio update.
pub fn audio_schedule_step<B: kira::backend::Backend>(
    manager: &mut AudioManager<B>,
    sources: &[AudioSchedule<'_>],
    listener: Option<(Entity, &Transform)>,
) -> Result<(), ManagerError>
where
    B::Error: std::fmt::Debug,
{
    // Listener pose first so the listener has a valid pose by the time
    // emitters compute their attenuation during Kira's process() call.
    if let Some((entity, transform)) = listener {
        let (_, _, _, listeners, anchor) = manager.parts_mut();
        if let Some(state) = listeners.get_mut(&entity) {
            state.sync_pose(transform, anchor);
        }
    }

    for sched in sources {
        if let Err(err) = reconcile_source(manager, sched) {
            // Don't abort the whole tick on a per-entity dispatch failure;
            // unknown clip is the only error class we surface — that's a
            // missing-asset bug and should be loud.
            if matches!(err, ManagerError::UnknownClip(_)) {
                return Err(err);
            }
        }
    }

    Ok(())
}

/// Reconcile a single source: pose, params, then state.
fn reconcile_source<B: kira::backend::Backend>(
    manager: &mut AudioManager<B>,
    sched: &AudioSchedule<'_>,
) -> Result<(), ManagerError>
where
    B::Error: std::fmt::Debug,
{
    // ---- pose ---------------------------------------------------------
    {
        let (_, _, sources, _, _) = manager.parts_mut();
        let Some(state) = sources.get_mut(&sched.entity) else {
            return Err(ManagerError::UnknownSource(sched.entity));
        };
        state.track.set_position(
            mint::Vector3 {
                x: sched.transform.position[0],
                y: sched.transform.position[1],
                z: sched.transform.position[2],
            },
            Tween::default(),
        );
    }

    // ---- play / pause / stop ----------------------------------------
    let desired = sched.source.desired_state;

    let (_, clips, sources, _, _) = manager.parts_mut();
    let state = sources
        .get_mut(&sched.entity)
        .ok_or(ManagerError::UnknownSource(sched.entity))?;

    if state.last_applied == desired {
        // State is already correct. Push parameter updates if they drifted.
        apply_params(state, sched.source);
        return Ok(());
    }

    match (state.last_applied, desired) {
        (_, PlaybackState::Stopped) => {
            if let Some(handle) = state.sound.as_mut() {
                handle.stop(Tween::default());
            }
            state.sound = None;
        }
        (PlaybackState::Stopped, PlaybackState::Playing | PlaybackState::Paused) => {
            // Need to spin up a fresh sound handle.
            let clip = clips
                .get(&sched.source.clip)
                .ok_or_else(|| ManagerError::UnknownClip(sched.source.clip.clone()))?;
            let mut data = clip
                .clone()
                .volume(amplitude_to_decibels(sched.source.volume))
                .playback_rate(PlaybackRate(f64::from(sched.source.pitch)));
            if sched.source.looped {
                data = data.loop_region(0.0..);
            }
            // Sounds in Kira 0.12 are routed via the spatial sub-track that
            // owns the emitter — `track.play(...)` rather than the old
            // `manager.play(data.output_destination(&emitter))` flow.
            let mut handle = state
                .track
                .play(data)
                .map_err(|err| ManagerError::Play(format!("{err:?}")))?;
            if matches!(desired, PlaybackState::Paused) {
                handle.pause(Tween::default());
            }
            state.sound = Some(handle);
            state.last_volume = sched.source.volume;
            state.last_pitch = sched.source.pitch;
            state.last_looped = sched.source.looped;
        }
        (PlaybackState::Paused, PlaybackState::Playing) => {
            if let Some(handle) = state.sound.as_mut() {
                handle.resume(Tween::default());
            }
        }
        (PlaybackState::Playing, PlaybackState::Paused) => {
            if let Some(handle) = state.sound.as_mut() {
                handle.pause(Tween::default());
            }
        }
        (PlaybackState::Playing, PlaybackState::Playing)
        | (PlaybackState::Paused, PlaybackState::Paused) => {}
    }

    state.last_applied = desired;
    apply_params(state, sched.source);
    Ok(())
}

/// Push out volume / pitch / loop changes if any of them drifted from the
/// last applied value. Cheap when nothing changed.
fn apply_params(state: &mut crate::source::SourceState, source: &AudioSource) {
    let Some(handle) = state.sound.as_mut() else {
        return;
    };

    if (state.last_volume - source.volume).abs() > 1e-4 {
        handle.set_volume(amplitude_to_decibels(source.volume), Tween::default());
        state.last_volume = source.volume;
    }
    if (state.last_pitch - source.pitch).abs() > 1e-4 {
        handle.set_playback_rate(PlaybackRate(f64::from(source.pitch)), Tween::default());
        state.last_pitch = source.pitch;
    }
    if state.last_looped != source.looped {
        if source.looped {
            handle.set_loop_region(0.0..);
        } else {
            handle.set_loop_region(None);
        }
        state.last_looped = source.looped;
    }
}

#[cfg(test)]
mod tests {
    use kira::backend::mock::{MockBackend, MockBackendSettings};
    use kira::AudioManagerSettings;

    use super::*;
    use crate::components::{AudioListener, AudioSource};
    use crate::manager::AudioManager;
    use crate::waveform::sine_wave;

    fn mock_mgr(sample_rate: u32) -> AudioManager<MockBackend> {
        AudioManager::<MockBackend>::with_settings(AudioManagerSettings {
            backend_settings: MockBackendSettings { sample_rate },
            ..Default::default()
        })
        .unwrap()
    }

    /// Schedule with no sources / no listener succeeds (no-op tick).
    #[test]
    fn empty_tick_is_noop() {
        let mut mgr = mock_mgr(48_000);
        audio_schedule_step(&mut mgr, &[], None).unwrap();
    }

    /// Stopped → Playing transitions create a sound handle.
    #[test]
    fn play_creates_sound_handle() {
        let mut mgr = mock_mgr(48_000);
        let samples = sine_wave(440.0, 48_000, 0.1);
        mgr.register_clip_from_samples("ping", 48_000, &samples);

        let listener_entity = Entity(1);
        let source_entity = Entity(2);
        let listener_xform = Transform::default();
        let source_xform = Transform::from_position([0.0, 0.0, -2.0]);

        mgr.register_listener(listener_entity, &listener_xform)
            .unwrap();
        let source = AudioSource {
            clip: "ping".into(),
            desired_state: PlaybackState::Playing,
            distances: (1.0, 100.0),
            ..AudioSource::default()
        };
        mgr.register_source(source_entity, &source_xform, &source)
            .unwrap();

        let sched = [AudioSchedule {
            entity: source_entity,
            transform: &source_xform,
            source: &source,
        }];
        audio_schedule_step(&mut mgr, &sched, Some((listener_entity, &listener_xform))).unwrap();
    }

    /// Unknown clip surfaces `ManagerError::UnknownClip`.
    #[test]
    fn unknown_clip_errors() {
        let mut mgr = mock_mgr(48_000);
        let entity = Entity(3);
        let xform = Transform::default();
        let source = AudioSource {
            clip: "missing".into(),
            desired_state: PlaybackState::Playing,
            ..AudioSource::default()
        };
        mgr.register_source(entity, &xform, &source).unwrap();
        let sched = [AudioSchedule {
            entity,
            transform: &xform,
            source: &source,
        }];
        let err = audio_schedule_step(&mut mgr, &sched, None).unwrap_err();
        assert!(matches!(err, ManagerError::UnknownClip(_)));
    }

    /// `AudioListener` default has unity gain so default world is audible.
    #[test]
    fn audio_listener_default_unity() {
        assert!((AudioListener::default().gain - 1.0).abs() < 1e-6);
    }
}
