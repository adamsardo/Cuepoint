# 03 Audio engine, decks, mixer and performance features

Status: draft, static evidence only (rekordbox 7.2.19 macOS).
Evidence sources: `classes.tsv` (C++ class and member inventory), `syms.demangled.txt` (signatures), `strings.txt` (UI and parameter strings, the embedded MIDI-learn command table, source paths), `imports.txt`, and the unpacked bundle under `Contents/`.
Evidence tags in backticks name a class, member, string or file. "Inferred" marks anything not directly shown by a symbol or string.

Conventions in this document:

- `rbxfrm::X` is the in-house engine framework ("RbxFrameworx" per source paths in `strings.txt`, for example `RbxFrameworx/Source/DJ/DjPlayer/Unit/DVS/DVSBehavior.cpp`).
- Unqualified classes such as `DjMixerUnit`, `EqIso`, `BeatFxEcho` live in the global namespace of the app binary.
- `djplay::`, `effectGui::`, `MergeFX::` are app-level controllers and panels that drive the engine.
- The UI build is JUCE 8 (`Unknown JUCE v8.` string, 183 `juce::AudioProcessorGraph` symbols). The engine is built as JUCE `AudioProcessor` nodes inside a `juce::AudioProcessorGraph`.

---

## 1. Purpose

The audio engine is everything between a decoded track and the sound card: up to four DJ decks with time-stretch and key control, cues, loops and performance pads, beat/BPM/key sync and Ableton Link, timecode vinyl (DVS), a four-channel DJ mixer with EQ/isolator, crossfader, headphones cue, mic and talkover, four effect families (Beat FX, Sound Color FX, Pad FX, Release FX) plus Merge FX and Smart CFX, a 16-slot sampler with step sequencer, Groove Circuit and Drum Swap built on stem playback, track-separation (STEMS) playback, mix and sequencer recording, Automix and Mix Point Link, and the device I/O layer (internal mixer mode vs external mixer mode, multi-device combining, routing to hardware channels).

For Cuepoint, this section is the parity checklist for `cuepoint-engine` (real-time graph, decks, mixer, sync, sampler, recording) and `cuepoint-dsp` (stretch, filters, effects). Stem separation (model inference) is owned by the analysis subsystem; this section only covers stem playback, per-stem gain, mute and FX.

---

## 2. Feature inventory (parity checklist)

### 2.1 Deck count, players and loading

- Four DJ decks in performance mode, each a `rbxfrm::DjPlayerUnit` wrapped by `rbxfrm::RekordboxDjPlayerUnit` (adds control-signal input and input-thru). `FourDeckSystem`, `DjSystem::getNumOfPlayer`, `rbxfrm::RekordboxDjPlayerUnit`
- Preview player (browser pre-listen) with its own output routing and volume. `DjPlayerControlForPreviewPlayerUnit`, `AudioGraphNodes::addPreviewPlayerNode`, `getChannelIndexOfPreviewPlayerInput`, `PreviewVolume` (SettingIF)
- Link monitor player (pre-listen of tracks on Pro DJ Link devices). `AudioGraphNodes::addLinkMonitorNode`, `UseLinkMonitor` (SettingIF), `DJSystemLinkMonitor`
- Track load, unload, load progress and "ready to play" before full decode (streaming decode with buffered ranges exposed to UI). `DjPlayerUnit::load`, `unload`, `DjPlayerUnitState::getBufferedRangesMs`, `isReadyToPlay`
- Load Lock (refuse load while playing), Needle Lock, load start position, load mode. `DeckLoadLock`, `NeedleLock`, `LoadStartPosition`, `LoadMode` (SettingIF), strings `Load Lock. Not available during playback.`
- Instant Doubles (clone another deck's track, position, play state, loop) including per-stem variant and auto instant doubles. `DjPlayerUnit::instantDouble`, `DjSyncSlaveBehavior::onInstantDoublesReq`, `AutoInstantDoubles`, `PartInstDoublesDirectionLeftRight`, string `STEM INSTANT DOUBLES`
- Sources: local files, streaming services (Beatport, SoundCloud, TIDAL, Spotify, Apple Music), cloud and protected "rbsc" files, iPod/iTunes, all behind a reader factory. `rbxfrm::AudioReaderFactory`, `StreamingReader`, `BeatportReader`, `SoundCloudReader`, `TidalReader`, `SpotifyReader`, `AppleMusicReader`, `FileReadRbsc`, `iPodReader`
- Play history registration (counts a track as played after a threshold or position move). `rbxfrm::PlayHistoryRegisterer`, `PlayedTime`, `PlayedOption` (SettingIF)
- Track-end handling: end-of-track flag, "on track end" warning. `DjPlayerUnitState::isOnTrackEnd`

### 2.2 Deck transport

- Play/pause, CUE (back-cue / cue standby), cue-play while held (cue sampler), jump to track start. `DjPlayerUnit::play`, `pause`, `backToCurrentCue`, `startCueSampler`, `DjPlayerState::isOnCueStandby`, MIDI `PlayPause`, `Cue`, `JumpToTrackStart`
- Play/pause with fade (short fade-out before pause, move or source change to avoid clicks). `AudioPlayerCore::PauseTaskOnFadeoutCompleted`, `MoveTaskOnFadeoutCompleted`, `getSamplesToFadeOut`, `f2dsp::Attenuator::letFadeIn/letFadeOut/letSmoothDiscontinuousPoints`
- Reverse play and slip reverse. `StretchBehavior::reqChangePlayDirection`, MIDI `Reverse`, `SlipReverse`
- Search forward/back (scan) with discrete scan speeds x5, x10, x20, x40, x60, x80, x120, x160. `rbxfrm::ScanBehavior`, `DjPlayerDef::SCAN_SPEED_x5` .. `SCAN_SPEED_x160`, `f2dsp::TimeStretchScan`
- Millisecond search / needle search (touch strip position jump). `ScanBehavior::reqMsSearch`, `DjPlayerUnit::msSearch`, `movePlayPositionRatio`, MIDI `NeedleSearch`
- Track search next/previous (previous/next track in list). MIDI `TrackNext`, `TrackPrev`
- Jog: vinyl mode on/off (scratch vs pitch bend), scratch with inertia, jog touch, jog speed, anti-drift. `DjPlayerUnit::scratch`, `stopScratch`, `enableVinyl`, `StretchBehavior::isScratchableByInertia`, `f2dsp::TimeStretchScratch`, `DeckJogSpeed`, `JogAntiDriftSetting` (SettingIF), MIDI `Vinyl`
- Pitch bend (nudge) by ratio offset and slope. `DjPlayerUnit::setBendDiff`, `setBendSlope`, `StretchBehavior::BendTaskOnPreRender`, MIDI `PitchBendUp/Down`
- Vinyl speed adjust (simulated turntable start and brake), separately for touch/brake and release/start, per side, with default acceleration tables. `rbxfrm::VinylSpeedAdjustor::accelerate/decelerate/setAccelerationByDefaultTable`, `DjPlayerUnit::adjustVinylSpeedByDefaultTable`, strings `Vinyl Speed Adjust`, `Touch/Brake`, `Release/Start`, MIDI `VinylAdjustBrake.Left/Right`
- Auto Cue (skip leading silence to first sound above a threshold level, per deck and all decks). `rbxfrm::AutoCueBehavior::reqSetAutoCueLevel`, `DjPlayerUnitState::getAutoCueLeveldB`, strings `Auto Cue Level`, MIDI `AutoCue`, `AutoCue4`
- Beat Jump by beats (pad values -1/2 .. +16 and more, pages and banks) and beat search. `rbxfrm::BeatSearchBehavior::getSamplePosBeyondBeats`, `DjPlayerUnit::beatSearch`, `pad/BeatJump.pad.csv`, MIDI `BeatJump`, `BeatJumpPage`, `BeatJumpBeat` (SettingIF)
- Slip mode: a background playhead keeps running during scratch, loops, hot cue hold, reverse, pause, slicer; release returns to it. `rbxfrm::SlipBehavior`, `rbxfrm::SlipBackGroundPlayer`, `SlipBehavior::triggerScratch/triggerReverse/triggerLaunchHotCue/triggerExitLoop`, `DjPlayerUnitState::getBackGroundPosMs`
- Quantize for cue/loop/hot cue actions, with a quantise beat value and modes (per deck, all decks in one go, sampler). `rbxfrm::QuantizedCueBehavior`, `DjPlayerQuantizePolicy`, `DjPlayerDef::QuantizeType`, `UnitOfTime::quantizePpq`, `DeckQuantize`, `DeckQuantizeBeat`, `DeckQuantizeMode`, `DeckQuantizeInOneGo` (SettingIF), string `Quantize Beat Value`
- Output wait state: actions arriving while the deck is not yet outputting audio are queued and some are disallowed. `rbxfrm::OutputWaitState::isHotCuePlayControlable` etc.
- Auto gain (track gain from analysis or tags; ReplayGain, iTunNORM, SeratoGain, MP3GAIN recognised), plus a "default gain" and anti-clipping gain. `rbxfrm::AutoGainAdjuster::calcNotClippingGain`, `getDefaultGain`, strings `REPLAYGAIN_TRACK_GAIN`, `ITUNNORM`, `SeratoGain`, `MP3GAIN`, `CurrentAutoGain` (SettingIF)
- Deck mute when the hardware mixer input selector or routing makes the deck inaudible (external input, sampler input, USB port, DDJ-RR selector, silent cue). `DeckMuteManager::checkMixerInputSelector/checkUSBPort/checkDDJRRSelector/enableSilentCue`, `rbxfrm::DjPlayerMuteUnit::mutePlayerIfNoOneControls`
- Jog Cutter (auto-generated quantised scratch and mute patterns while the jog is touched). `djplay::JogCutterManager::executeJogCutter`, `rbxfrm::AutoScratchUnit` (ScratchTables, JogtouchTables), `rbxfrm::AutoMuteUnit` (MuteTables), `setHalveBpmIfAboveThreshold`, string `kJogCutterDisableByDVS` (inferred link between Jog Cutter and AutoScratch/AutoMute)
- Sample Scratch (pads load sampler slots into a deck for scratching, eject). `SamplerControlPanel::SampleScratch`, MIDI `SampleScratch`, `pad/SampleScratch.pad.csv`
- Slicer capture (capture a loop or slicer section into sampler slots). `SamplerControlPanel::deviceSlicerCapture`, `deviceLoopCapture`, MIDI `SlicerCapture`, `SlicerCaptureSaveBank` (SettingIF), string `Invalid operation. Empty 8 slots for Slicer Capture.`

### 2.3 Cues and loops

- Memory cues (set, delete, call next/previous), memory cue call lock, max memory cue count. `CueBehaveDispatcher::reqSetCurrentCue`, MIDI `Memory`, `Delete`, `Parameter1Right_HotCue`, `MemoryCueCallLock`, `MaxMemoryCueNumber` (SettingIF)
- Hot cues A to P (16) with record, launch, stop on release (hot cue sampler mode), delete, hot loops, hot cue auto load, hot cue colours, count-down display. `DjPlayerUnit::launchHotCue/recHotCue/stopHotCue/deleteHotCue/enableHotCueSampler`, `pad/HotCue.pad.csv` values A..P, `MaxHotCueNumber`, `HotCueAutoLoad`, `HotCueCountDownMode` (SettingIF)
- Hot cue launch with sync request (launch quantised and phase-aligned to master). `SyncHotCueLaunchTrigger`, `BpmSyncBehavior::onHotCueLaunchWithSyncReq`
- Loop In, Loop Out, Reloop/Exit, Loop re-trigger, loop half/double, loop move, loop in/out adjust, loop cut. `CueBehaveDispatcher::reqStartLoop/reqReloop/reqLoopMove/reqAdjustLoopPoint/reqAdjustLoopLength/reqLoopCut`, MIDI `LoopIn`, `LoopOut`, `ReloopExit`, `ReTrigger`, `LoopHalf`, `LoopDouble`, `LoopMove*`
- Auto beat loop 1/64 to 512 beats (MIDI table) and pad Beat Loop mode, long and short variants, auto loop beat default. `QuantizedCueBehavior::doShortAutoBeatLoop/doLongAutoBeatLoop`, MIDI `BeatLoop1_64` .. `BeatLoop512`, `AutoLoopBeat` (SettingIF), `pad/BeatLoop.pad.csv`
- Auto beat loop start mode (loop starts at the press point vs grid). `DjPlayerUnit::enableAutoBeatLoopStartMode`
- Slip (instant) loop and slip beat loop. `DjPlayerUnit::instantSlipLoop`, `exitInstantSlipLoop`, `CueBehaveDispatcher::reqSlipBeatLoop`, `SyncSlipBeatLoopTrigger`
- Active loop (a stored loop that engages automatically when the playhead reaches it), select/off for hot cue or memory cue loops. `rbxfrm::ActiveLoopBehavior`, MIDI `ActiveLoop`, `ActiveLoopSelectHotCue`, `ActiveLoopOffMemCue`
- Loop with key shift (cue/loop launched with a pre-set key shift). `CueBehavior::CueWithKeyShiftTaskOnWarped`, `LoopWithKeyShiftTaskOnWarped`
- Minimum loop length and valid auto loop beat registry. `CueBehavior::getMinimumLoopLength`, `DjPlayerUnit::registValidAutoLoopBeats`
- Cue buffer: a decoded RAM buffer around each cue (forward and reverse sizes) so hot cues start instantly. `rbxfrm::CueBufferController::ensureCueBuffer`, `ReaderOwnerTmpl::CUE_BUFFER_SIZE_FWD/REV`, `DjPlayerConfig::getCueBufferRange`
- Seamless looping via "warp points" (sample-accurate jump points the reader follows). `IStereoAudioSource::setWarpPoint/getWarpExit/clearOnceOnlyWarpPoint`, `IWarpListener`
- Active Censor (mark in/out ranges in a track that are automatically covered by an effect: REV ROLL, TRANS, ECHO, V.BRAKE, with beat value). `rbxfrm::ActiveCensorBehavior`, `IDjPlayerActiveCensor::EffectType`, `pad/ActiveCensor.pad.csv`, MIDI `ActiveCensor*`
- Slicer and Slicer Loop: 8 slices over a domain, unit width, roll width, shift domain, slicer roll up/down. `rbxfrm::SlicerBehavior` (setMode, setUnitNum, setUnitWidth, shiftDomain, isRolling), `IDjPlayerSlicer::Mode`, MIDI `Slicer`, `SlicerLoop`, `Parameter1Right_Slicer`
- Key Shift pads (semitone offsets -8..+7 per pad page) and Keyboard mode (play a hot cue chromatically from pads). `pad/KeyShift.pad.csv`, `pad/Keyboard.pad.csv`, MIDI `KeyShift`, `Keyboard`
- Sequence Call / Sequence Load from pads (see sampler). `pad/SequenceCall.pad.csv`
- Pad mode catalogue: HOT CUE, PAD FX1, BEAT JUMP, SAMPLER, KEYBOARD, KEY SHIFT, BEAT LOOP, PAD FX2, SLICER, ACTIVE CENSOR, SEQUENCE CALL, PAD FX Combination, LIGHTING, TRANSPORT, SP.SCRATCH, STEMS, USER 1..8, Combination 1..9. `pad/Mode.pad.csv`
- Transport pad mode actions: PREVIOUS TRACK, NEXT TRACK, SLIP, TEMPO RESET, BPM -/+ MEDIUM, CUE/LOOP CALL < >, PLAY/PAUSE, CUE, PITCH BEND -/+. `pad/Transport.pad.csv`
- Mix Point Link: set mix-out and mix-in points across two decks; at the mix-out point the incoming deck auto-starts from its mix-in point, with optional BPM sync auto change, silent edit mode and target deck select. `djplay::MixPointLinkComp`, `rbxfrm::DjPlayerMixPointLink::mixPointLinkPlayStart`, `DjPlayerUnit::mixPointLinkControlStart`, `isMixPointLinkWaitingAutoStart`, `MixPointLinkBpmSyncAutoChange` (SettingIF), MIDI `MixPointLink*`; not available with Automix or Ableton Link (`kMixPointLinkErrorAutomix`, `kMixPointLinkErrorAbletonLink`)

### 2.4 Tempo, key and time-stretch

- Tempo slider with ranges ±6, ±10, ±16 and WIDE (±100 %), tempo reset, tempo takeover. `"+/-6"`, `"+/-10"`, `"+/-16"` in `english.lang`, strings `Out of range. BPM Sync is not available because Tempo Range exceeds 100%.`, MIDI `TempoRange`, `TempoReset`, `TempoUp/Down`
- Base tempo ratio and set tempo by BPM. `StretchBehavior::reqSetBaseTempoRatio`, `StretchablePlayerCore::doSetXSpeedByBpm`
- Master Tempo (key lock) on/off, with a selectable master-tempo type (int enum, values not in strings). `DjPlayerUnit::enableMasterTempo`, `StretchablePlayerCore::reqSetMasterTempoType(int)`, `MasterTempoEnabled` (SettingIF), MIDI `MasterTempo`
- Time-stretch engine modes (`f2dsp::TimeStretchMode`, values unknown): high-quality phase-vocoder-style stretch ("FGPR": FFT, Hanning window, phase modification to complex), a "simple" cross-fade (time-domain overlap) key lock used as fallback when the HQ look-ahead buffer is exhausted, a scratch path and a scan path. `rbxfrm::dsp::TimeStretchFGPR`, `dsp::FGPRImpl::modifyPhaseToComplex`, `dsp::HanningWindow`, `dsp::Fft`, `dsp::SimpleCrossFade`, `dsp::CrossFadeTimeScaling`, `TimeStretchFGPR::operateSimple/operateReal/operateSimpleToReal/operateRecover`, `isExhaustedKeyLockBuffer`, `isProcessingSimpleKeyLock`, `f2dsp::TimeStretchManager`, `f2dsp::TimeStretchScratch`, `f2dsp::TimeStretchScan`
- High-quality resampler for non-key-locked speed change and sample-rate conversion (Kaiser-windowed sinc, convolution with fine and normal paths). `rbxfrm::f2dsp::SamplingFunctionSRC`, `f2dsp::c_src::init_kaiser`, `convolute_fine`, `convolute_normal`, `rbxfrm::SampleRateConverter`
- Key shift by semitone (independent of tempo), key reset, key shown relative to original, "diff semitone on tempo ratio" (displayed key drift when master tempo is off). `DjPlayerUnit::shiftPlayKeySemitone`, `resetPlayKey`, `dsp::PitchTimeStretchFGPR::setPitch`, `StretchBehavior::createSemitoneConvertTable`, `DjPlayerUnitState::getDiffSemitoneOnTempoRatio`, MIDI `SemitoneUp/Down`, `KeyReset`, `KeyShowOriginalState` (SettingIF)
- Key Sync (match key to the key-sync master: master chosen by on-air priority, play priority, deck priority; per-deck state; harmonic matching on Camelot/alphanumeric codes). `rbxfrm::KeySyncManager::decideKeySyncMaster/isKeyMatching`, `DjKeySyncMasterBehavior::getOnAirPriorityAsKeySyncMaster`, `DjKeySyncSlaveBehaveDispatcher`, `KeySyncBehavior::getSyncKeyCode`, `KeyUnsyncableBehavior`, `IKeyInfo::KeyCode` (24 keys plus `stringAlphanumericKey1A..12B`), MIDI `KeySync`
- Separate pitch shifter used by effects (pitch shift with cross-fade and mute at max pitch down). `rbxfrm::PitchShifter::processPitchShift`, `enableMuteWhenPitchDownMax`
- Tap BPM. `rbxfrm::TapBPMDetector`, MIDI `TapBpm`
- Slow playback filter (smoothing when playing very slowly, inferred anti-aliasing for scratch). `SlowPlaybackFilter::makeCoefTable`

### 2.5 Sync

- Sync on/off, sync master selection (explicit "Master" button or automatic by priority), sync master can be a deck, the sampler/sequencer, Ableton Link, or an internal clock. `rbxfrm::SyncManager::decideSyncMaster`, `shouldChangeMasterByPriority`, `ISyncMaster` implementers `DjSyncMasterBehavior`, `AbletonLinkUnit`, `FlexibleClockGenerator`, `SimpleClockGenerator`, `LocalSyncManager`, MIDI `Sync`, `Master`
- Two sync policies: Beat Sync (tempo plus phase on beat grid) and BPM Sync (tempo only), with an option to auto-switch between them. `DjPlayerDef::BeatSyncPolicy`, `BeatSyncBehavior`, `BpmSyncBehavior`, `BeatSyncType` (SettingIF), string `Automatically changes the BEAT/BPM SYNC setting`
- Slave behaviour state machine: BeatSync, BpmSync, DvsBpmSync, OutOfRange (BPM outside tempo range), Unsyncable (no grid, one-shot), Dummy. `DjSyncSlaveBehaveDispatcher::createBeatSyncBehavior/createBpmSyncBehavior/createOutOfRangeBehavior/createUnsyncableBehavior`, `DjSyncSlaveBehavior-in-rbxfrm`
- Phase alignment computed in PPQ against the master playhead; minimum jump to sync at head. `DjSyncSlaveBehaveDispatcher::calcPhaseDiffPpq`, `SlavePlayerFunctions::getDiffPpqWithMaster`, `getMinJumpSamplesToSyncAtHead`, `adjustXSpeedToSyncBeat`, `adjustXSpeedToSyncBpm`
- Sync rate (half/double tempo matching, toggle, limited sync rate), multi-BPM sync, display of sync rate. `DjPlayerUnit::toggleSyncRate`, `resetToggleSyncRate`, `setLimitedSyncRate`, `enableMultiBpmSync`, `MultiBpmSyncMode` (SettingIF), string `Display SYNC RATE`
- Sync-aware triggers: play start, play position, hot cue launch, auto beat loop, slip beat loop, loop cut. `SyncPlayStartTrigger`, `SyncPlayPositionTrigger`, `SyncHotCueLaunchTrigger`, `SyncAutoBeatLoopTrigger`, `SyncSlipBeatLoopTrigger`, `SyncLoopCutTrigger`
- Ableton Link: enable, tempo, quantum, phase, peer count, start/stop sync, metronome with volume, tap BPM and BPM up/down. `rbxfrm::AbletonLinkUnit` (setQuantum, enableStartStopSync, getLinkNumPeer, renderMetronomeIntoBuffer), 703 `ableton::link` symbols (statically linked Link library), `AbletonLinkStartStopSyncEnabled` (SettingIF), MIDI `AbletonLinkTapBpm`, `AbletonLinkBpmUp/Down`
- Metronome (three click sounds, volume, lock owner). `rbxfrm::MetronomeUnit`, `Resources/binary/Click Sound 01 Electronic.wav`, `Click Sound 02 HiHat.wav`, `Click Sound 03 Cowbell.wav`, `MetronomeEnabled`, `MetronomeVolume`, `SelectMetronome` (SettingIF)
- Real-time BPM detection of external audio (input thru, master, sampler, send/return FX) for FX timing. `rbxfrm::AutoBPMDetectorUnit`, `AutoBPMCalculator`, `DjSystem::getInputThruBPMManager`, `AudioGraphNodes::addSamplerAutoBpmNode`, `addSendReturnFxAutoBpmNode`, `DjMixerUnit::getMasterBPMDetectorUnit`
- BPM manager for FX (auto BPM vs tap BPM, BPM range and time signature limits). `rbxfrm::BPMManager::enableAutoBpm/limitBpmRange/limitTimeSigniture`, MIDI `FX1AutoBeat`, `FX1TapBeat`
- Pro DJ Link tempo master exchange (with CDJs/DJMs). `PSvLinkSyncMasterReqSndInfo` (owned by the Pro DJ Link subsystem; engine must expose master state)

### 2.6 DVS (timecode control)

- Control media: rekordbox Control Vinyl (1 kHz carrier) and rekordbox Control CD/WAV (1 kHz); disc sides detected. `strings: rekordbox Control Vinyl 1kHz`, `rekordbox Control wav 1kHz`, `rbxfrm::DVSDiscInfo::DiscSide`, `DVSSignalDecoder::DiscSideDetecting`, `TimecodeInfo::Templates::createTimecodeInfo(DVSControlDiscSideType)`
- Control modes: Absolute, Relative, Internal, Thru, Wireless (A/B), toggle INT vs WIR/REL/ABS, temporary internal mode, emergency internal mode on signal loss. `DVSBehavior::isAbsoluteMode/isRelativeMode/isInternalMode/isThroughMode/isWirelessMode`, `DVSEmergencyInternalMode`, `rbxfrm::DVSControlMode`, MIDI `DvsAbsolute`, `DvsRelative`, `DvsInternal`, `DvsThrough`, `DvsWireless(A)`, `DvsWirAbsRelInt`
- Decoder: zero-crossing, polarity detection, position detection by level difference and peak average, pitch detection, move and position quality metrics. `DVSSignalDecoder`, `TimecodeCorrectPolarityDetect`, `ZeroCrossDetect`, `PositionDetectLevelDiff`, `PositionDetectPeakAverage`, `PitchDetect`, `DVSControlSignalState::getMoveQuality/getPositionQuality`
- Calibration: auto calibration, noise calibration, L/R balance, phase balance, noise level; per-input timecode source (PHONO, CD/LINE, DIGITAL, LINE). `DVSBehavior::startAutoCalibration/startNoiseCalibration`, `TimeCodeCalib_LRBalance`, `TimeCodeCalib_PABalance`, `TimeCodeCalib_NoiseLevel`, `TimeCodeSource` (SettingIF), strings `CH1 Timecode PHONO` .. `CH4 Timecode LINE`
- Needle drop (absolute position jump), jump to song head when needle is in the lead-in. `DVSNeedleDrop::processJumpToHead`, `DvsVinylLeadInSongHead` (SettingIF)
- Relative pitch with reference hardware RPM (33/45 inferred), cancel relative pitch on BPM sync, DVS BPM sync behaviour. `DVSRelativePitch::setReferenceHWRPM`, `DvsBpmSyncBehavior`, `kDvsBpmSyncErrorInPause`
- Strict scratch judgement from timecode statistics. `rbxfrm::ScratchJudgement`, `StatisticsBuffer`, `DjPlayerState::isPlayingNormalyWithDVSStrictScratchJudge`
- Noise-cut high-pass on timecode input. `rbxfrm::NoiseCutHPF`
- Signal scope rendering for the control signal panel. `SignalScopeRenderThread`, `SignalScopeZoomRate` (SettingIF)
- DVS routing UI per device. `DVSAudioRoutingComponent`, `DVSRoutingSetting`

### 2.7 Mixer (internal mixer mode)

- Per channel: trim (with headroom in dBFS) or auto gain, 3-band EQ or isolator with kill, channel fader with selectable curve (including long-fader curve), crossfader assign A/THRU/B, channel cue, level meter (pre and post fader), mute, per-channel Color FX knob. `InputChannelGraph::addTrim/addEqIso/addChannelFader/addCrossFaderAssign/addLevelMeter/addOutLevelMeter/setMute`, `getTrimHeadRoomdBFS`, string `Kill Switch:`, `MySettingCHFaderCurveComponent`, `MySettingCHFaderCurveLongFaderComponent`
- EQ variants: default equaliser, DJM-900NXS2 modelled equaliser, isolator; EQ vs ISO mode; type selection. `Equalizer`, `EqualizerNXS2`, `Isolator`, `EqIso::processBlock`, `EqualizerMode`, `EqualizerType` (SettingIF), string `EQ/ISOLATOR`
- STEM ISO mode: the EQ knobs become per-stem level knobs (Inst/Bass, Vocal, Drums) with takeover. `PartIsoControlComp`, `StretchablePlayerCore::reqSetPartIsoValue`, `partIsoCurve`, MIDI `PartIsolator*`, string `Takeover when switching to EQ/STEM ISO`
- Crossfader with curves (enum `ECrossFaderCurve_t`), cut lag, reverse. `CrossFaderVolumeControl::setCurve(ECrossFaderCurve_t,int)`, `ControllerCrossFaderCutLagComponent`, `ControllerCrossFaderReverseComponent`, `MySettingCrossFaderCurveComponent`
- Smart Fader (one fader controls volume, EQ/filter and tempo of outgoing and incoming decks, uses envelope generators). `djplay::SmartFader::updateFaderCurve`, `setTempoRange`, `FaderForSmartFader` (SettingIF), `DjMixerUnit::getChannelEnvelopeGeneratorArray`, `EnvelopeGenerator` (inferred link)
- Fader Start (channel fader or crossfader starts/returns the deck). `ControllerFaderStart`, string `Fader Start`
- Master: master volume, master limiter, master level meter, master out stereo/mono, second master out, booth out with limiter and stereo/mono, link out, record out with delay compensation. `OutputChannelGraph::enableMasterLimitter/enableBoothLimitter/enableMasterOutStereo/enableBoothOutStereo`, `getChannelIndexOfMasterOut2`, `getChannelIndexOfLinkOut`, `DjMixerUnit::setRecOutDelayTimeMs`
- Headphones: cue per channel, master cue, sampler cue, cue mix (cue/master balance), headphones volume, cue mode, pre-EQ setting, mono split. `DjMixerUnit::setChannelCue/setMasterCue/setSamplerCue/setHeadPhoneMixingBalance/setHeadPhoneVolume/selectHeadPhoneCueMode`, `MySettingHeadphonesPreEQComponent`, `MonoSplitOn` (SettingIF), `rbxfrm::MonitorCue`
- Mic: on/off, level, 3-band mic EQ, low-cut, mic FX (selector, low latency mode), feedback eliminator (types), talkover normal and advanced with level, mic to booth and rec out, mic limiters. `MicChannelGraph`, `MicEqualizer`, `MicInputFilter`, `MicFxSelector`, `MicFeedbackEliminator::setType`, `TalkOver::setTalkOverMode/setTalkOverLevel`, `TALKOVER_LEVEL_TABLE`, `DjMixerUnit::enableAdvTalkover/connectMicToBoothOut/connectMicToRecOut/enableMicLimitterForMaster`, strings `TALKOVER`, `ADVANCED`, `NORMAL`
- Sampler channel: sampler volume and gain, sampler routed to master or into a deck channel, sampler cue, sampler FX. `SamplerInputChannelGraph`, `DjMixerUnit::switchSamplerRouting`, `isUsingSamplerToCHRouting`, `setSamplerGain`
- Send/return (aux) FX to external hardware FX, 4-channel variant. `DjSystem::setSendReturnFx`, `effectGui::FxSendReturnManager`, `DeviceDefs::has4chSendReturnFx`, `SendReturnFxEnabled` (SettingIF)
- On-air detection per channel and sampler (used by key sync priority, lighting, FX). `DjMixerUnit::isOnAir/isNowOnAir/isSamplerOnAir`, `rbxfrm::OnAirStateServer`
- Level meters with peak hold and smoothing. `rbxfrm::GeneralLevelMeter`, `PeakDetectorUnit`
- Mixer state persisted to DB. `rb::middleware::db::rekordbox::MixerParam`

### 2.8 Mixer modes and audio I/O

- Mixer mode Internal vs External (hardware mixer); switching stops Automix; some devices fixed. `DjMixerUnit::isInternalMode`, `DjSystem::isInternalMode`, `AudioMixerMode`, `DeviceDefs::getDefaultMixerMode/getFixedMixerMode`, strings `Mixer Mode`, `Automix will stop when switching to External Mixer mode.`, `MixerInternalMode` (SettingIF)
- External mode: each deck to its own hardware output pair; sampler and preview to dedicated outputs; external rec input. `DJSystemRoutingManager::setExternalPlayerOutChannelId/setExternalSamplerOutChannelId/setExternlPreviewOutputCh/setExternalRecInputChannelId`
- Internal mode: master, cue (headphones), booth and rec channel ids; mic input ids; THRU input (external audio into a deck channel). `DJSystemRoutingManager::setInternalMasterOutChannelId/setInternalCueOutChannelId/setInternalBoothOutChannelId/setTHRUInputChannelId/setMicInputChannelId`
- Input Thru (deck channel plays an external input instead of a track, e.g. PHONO/LINE). `DjSystem::enableInputThru`, `RekordboxDjPlayerUnit::enableInputThru`
- Device layer: fork of JUCE AudioDeviceManager with CoreAudio device, device combiner (separate input and output devices as one), binder, xrun counting, CPU usage, audio workgroups, buffer size and sample rate choice, channel names, latencies. `rb::AudioDeviceManager`, `rb::CoreAudioClasses::CoreAudioIODevice`, `AudioIODeviceCombiner`, `AudioIODeviceBinder`, `getWorkgroup`, `getXRunCount`, `rb::CallbackMaxSizeEnforcer`
- Device type factories for CoreAudio, ASIO, WASAPI, ALSA, JACK (macOS uses CoreAudio). `rb::AudioIODeviceType::createAudioIODeviceType_CoreAudio/_ASIO/_WASAPI`
- Audio settings: device, buffer size, sample rate, output channel count, input channels, PC master out device, PC speaker. `AudioBufferSize`, `SampleRate`, `OutputChannelNum`, `InputChannels`, `AvailablePCMasterOutDevices`, `PCSpeakerSelected` (SettingIF)
- Hardware-specific behaviours from a device table (mic count, send/return channels, rec-only devices, mono split, motorised jog, low-precision DVS). `djplay::DeviceDefs` (getNumMics, getNumSendReturnChannels, isRecOnlyDevice, isPioneerMonoSplitDevice, isMotorizedJogDevice, isLowPrecisionDvsDevice)
- Hardware DSP Beat FX state (when the controller does Beat FX in hardware the software unit is shaded). `effectGui::HardwareDSPBeatFXState`
- Audio device watcher (hot-plug). `djplay::AudioDeviceWatcher`, `RemovableDeviceDetector`
- Fixed internal processing sample rate with SRC to/from the device rate (inferred from class name; rate value unknown). `rbxfrm::FixedFsAudioProcessor::prepareSRCModules/convertInputSamples/convertOutputSamples`, `UnitOfTime::defaultSampleRate`, `rawSampleRate`

### 2.9 Effects

All effect units share a common interface: select effect, beat value (fraction of a beat), effect time in ms, level/depth, numbered parameters with ranges, steps, default and text, on/off, channel assignment, BPM source, tail clearing. `AbstractFxUnit` (selectEffect, setEffectBeat, setEffectTimeMs, getParameterRange, getParameterNumSteps, getParameterText, setBeatPerMitunes, enableFxToChannel), `EffectBase` (processSubsonicHPF, setInOutGainInfo, getPureEffectMixRatio, setEffectBeatTimeWithPlayHead), `EffectInOutGain`, `FxQuantizeManager` (quantised on/off and retrigger on beat).

#### 2.9.1 Beat FX (two units FX1, FX2; multi mode = 3 effects in series with one parameter each; single mode = 1 effect with up to 5 parameters)

- Units, modes and controls: FX1/FX2, multi/single toggle, per-slot level/depth, on, select next/back, beat up/down, beat size, adjust time, BPM auto or tap, assign to deck 1..4, sampler and master, release FX per unit. MIDI `FX1-1` .. `FX2-3`, `FX1FXMode`, `FX1AdjustTime`, `FX1Assign1..4`, strings `Beat FX multi-mode: This mode allows to use up to three effects simultaneously.`, `Beat FX single-mode: This mode allows to use one effect with five parameters.`, `FX assign (Sampler):`, `FX assign (Master):`, `RekordboxMultiFxUnitContainer`, `RekordboxSingleFxUnitContainer`, `RekordboxDJMultiFxLayer`, `RekordboxDJSingleFxLayer`
- Effects present as both single and multi implementations (user-facing names from strings where present, otherwise from class name):
  - DELAY `SingleBeatFxDelay`, `MultiBeatFxDelay`
  - ECHO `SingleBeatFxEcho`, `MultiBeatFxEcho`, `EchoCore`
  - PING PONG `SingleBeatFxPingPong`, `MultiBeatFxPingPong`, string `PING PONG`
  - SPIRAL `SingleBeatFxSpiral`, `SpiralCore`, `SpiralBuffer` (SPIRAL_MODE)
  - REVERB `SingleBeatFxReverb`, `MultiBeatFxReverb`, `ReverbCore`, `NewReverbCore`, `ReverbCombFilter`, `ReverbAllPassFilter`
  - TRANS `SingleBeatFxTrans`, `MultiBeatFxTrans`, `TransCore`
  - FILTER `SingleBeatFxFilter`, `MultiBeatFxFilter`, `LFOFilterCore`
  - FLANGER `SingleBeatFxFlanger`, `MultiBeatFxFlanger`, `FlangerCore` (strings `FLANGER RESONANCE`, `FLANGER MAX DELAY TIME MS`)
  - PHASER `SingleBeatFxPhaser`, `MultiBeatFxPhaser`, `PhaserCore` (strings `Stages Lch`, `Fc Peak`, `Fc Bottom`)
  - PITCH `SingleFxPitch`, `MultiFxPitch`, `PitchShifter`
  - SLIP ROLL `SingleBeatFxSlipRoll`, `MultiBeatFxSlipRoll`, `SlipRollCore`
  - ROLL `SingleBeatFxRoll`, `MultiBeatFxRoll`, `RollCore`, `RollEffectMixRatioCurveTable`
  - REV ROLL `SingleBeatFxReverseRoll`, `MultiBeatFxReverseRoll`, `ReverseRollCore`
  - VINYL BRAKE `SingleBeatFxVinylBrake`, `MultiBeatFxVinylBrake`
  - HELIX `SingleBeatFxHelix`, `MultiBeatFxHelix`, `HelixCore` (strings `BASE DELAY TIME(ms)`, `1ST DELAY GAIN` .. `8TH DELAY GAIN`, `LEFT SWING`, `RIGHT SWING`)
  - MOBIUS (SAW, TRI) `SingleBeatFxMobiusSaw`, `SingleBeatFxMobiusTri`, `MultiBeatFxMobiusSaw/Tri`, strings `MOBIUS SAW`, `MOBIUS TRI`
  - ENIGMA JET `SingleBeatFxEnigmaJet`, `MultiBeatFxEnigmaJet`
  - STRETCH `SingleBeatFxStretch`, `MultiBeatFxStretch`, `StretchCore`
  - PAN `SingleBeatFxPan`, `MultiBeatFxPan`
  - REV DELAY `SingleBeatFxReverseDelay`, `MultiBeatFxReverseDelay`, string `REV DELAY`
  - PITCH ECHO, LOW CUT PITCH ECHO `SingleBeatFxPitchEcho`, `SingleBeatFxLowCutPitchEcho`, `PitchEchoCore`, `PitchReverseEchoCore`
  - LOW CUT ECHO `SingleLowCutFxEcho`, `MultiLowCutEcho`
  - MULTI TAP DELAY / MT DELAY `SingleMultiTapDelay`, `GroupMultiTapDelay`, strings `MULTI TAP DELAY`, `MT DELAY`
  - UP ECHO, DOWN ECHO (multi only) `MultiBeatFxUpEcho`, `MultiBeatFxDownEcho`
  - ROBOT `SingleFxRobot`, `MultiFxRobot`, `RobotCore`
  - FX+ family: BPF ECHO, HPF ECHO, LPF ECHO, CRUSH ECHO, NOISE ECHO, REVERB UP, REVERB DOWN, SPIRAL UP, SPIRAL DOWN `SingleFxPlusBpfEcho` .. `SingleFxPlusReverbDown`, `MultiFxPlus*`, strings `FXP BPF ECHO`, `FXP REVERB UP`, `SPIRAL UP`, `SPIRAL DOWN`
- Parameter names seen in strings (subset, used to label parameters): `ECHO TIME`, `Feedback`, `R RATIO`, `LR DELAY`, `REVERSE LFO`, `CUTOFF FREQ`, `RESONANCE GAIN`, `ECHO FREEZE`, `PITCH(%)`, `REVERB TIME(Ms)`, `ROOM SIZE(%)`, `LFO CYCLE Ms`, `LFO TYPE`, `DELAY TIME(ms)`, `REVERSE FB`, `TIME TRIM`, `TRACK SPEED`. `strings.txt` lines near `MULTI ECHO`
- Release FX in Beat FX unit: VINYL BRAKE, BACK SPIN, ECHO with per-type beat values, release FX unit number, release FX on Color FX. `RekordboxReleaseFxUnitContainer`, `RekordboxDjReleaseFxLayer`, `ReleaseFxVinylBrake`, `ReleaseFxBackSpin`, `ReleaseEcho`, `RfxVinylBreakBeatValue`, `RfxBackSpinBeatValue`, `RfxEchoBeatValue`, `ReleaseFxUnitNum`, `RelaseFxOnCfx` (SettingIF), strings `RELEASE(VINYL BRAKE)`, `RELEASE(ECHO)`, `RELEASE(BACK SPIN)`

#### 2.9.2 Sound Color FX (CFX, one knob per channel plus sampler and master, one parameter knob)

- Structure: select CFX, on, per channel knob CH1..CH4, sampler and master (centre = off, bidirectional), parameter knob, CFX on sampler setting, single and multi CFX layouts. MIDI `CFXParameterCH1..4`, `CFXParameterSampler`, `CFXParameterMaster`, `CFXOn`, `CFXSelect`, strings `COLOR Knob (Channel):`, `If the COLOR knob is at the center position, the FX is not applied.`, `RekordboxDjColorFxUnitContainer`, `RekordboxDjColorFxLayer`, `MultiCFX`, `SingleCFXDefault`, `CFxOnSampler` (SettingIF)
- Effects (class names; user names FILTER, DUB ECHO, GATE COMP, SPACE, NOISE, CRUSH, PITCH, JET, SWEEP, ECHO, REVERB confirmed or inferred): `ColorFxFilterDjm900NXS2` (FILTER, DJM-900NXS2 modelled cutoff table), `ColorFxDubEcho` (`DUB ECHO`), `ColorFxEcho`, `ColorFxGateComp` (`GATE COMP`, params `THRESH`, `ATTACK`, `COMP_RATIO`, `GATE_RELEASE`), `ColorFxSpace` (`SpaceCore`, all-pass and comb network), `ColorFxNoise` / `ColorFxNoiseDjm900Nxs2` (`WhiteNoiseOSC`), `ColorFxCrush` (`CrushCore`), `ColorFxPitch`, `ColorFxJetRZX` (JET, modelled on DJM-RZX inferred from name), `ColorFxSweep`, `ColorFxReverb`, `ColorFxAutoTune` (string `CFX AUTO TUNE`)
- Smart CFX: one knob drives a preset combination; presets PHANTOM ECHO, REFLECT ECHO, MOBIUS ECHO, VAPORIZE, NOISE CHOPPER, CYBER JET, CYBER PITCH, TWISTER; not with Touch FX. `djplay::SmartCFX` (SMART_CFX_PRESET_LIST), `SmartCfxPreset`, `SmartCfxPresetPosition` (SettingIF), strings listed, `SMART CFX mode cannot be turned ON while Touch FX is ON.`

#### 2.9.3 Pad FX

- Two Pad FX banks (PadFx1, PadFx2) of 16 pads each, plus Pad FX Combination presets; each pad holds an effect from Beat FX, Release FX, Sound Color FX or Remix (RMX) lists with beat/level/parameter, hold type; press to engage, release to stop (with optional release FX); beat shift; edit mode; beginner, standard and combination presets. `djplay::PadFxView` (getPadFxBeatFxList, getPadFxReleaseFxList, getPadFxSoundColorFxList, getPadFxRmxFxList, getPitchShiftList, getRoomSizeList, PfxHoldTypeName), `djplay::PadFxSettings::padFxBeginnerPreset/padFxStandardPreset/padFxCombinationPreset`, `RekordboxDjPadFxLayer`, `pad/PadFx1.pad.csv`, `pad/PadFxCombination.pad.csv`
- Pad FX release FX names: VINYL BRAKE, BRAKE ECHO, ECHO, REVERB, BACK SPIN, SPIN ECHO. strings `[ RELEASE FX ] VINYL BRAKE` .. `[ RELEASE FX ] SPIN ECHO`, `ReleaseFxBackSpinEchoOut`, `ReleaseFxVinylBrakeEchoOut`, `BackSpinCore`, `BackSpinReverseRoll`, `ReleaseInputGate`, `ReleasePitchEcho`
- Pad FX also uses legacy beat FX cores (inferred from a third, unprefixed family): `BeatFxDelay`, `BeatFxEcho`, `BeatFxEnigmaJet`, `BeatFxFilter`, `BeatFxFlanger`, `BeatFxHelix`, `BeatFxMobius`, `BeatFxPan`, `BeatFxPhaser`, `BeatFxPitchEcho`, `BeatFxReverb`, `BeatFxSpiral`, `BeatFxStretch`, `BeatFxTrans`, `BeatFxVinylBrake`, `BeatFxPingPongThunk`, plus SHORT ECHO and DUBVERB strings
- Remix (RMX) style FX available to pads (inferred mapping): `RmxSpiral` (RMX SPIRAL UP/DOWN), `Rmx4thEcho`, `DryCutSpiral`, `SpotDub` (`SPOT DUB`), `InstFxHPF`, `InstFxLPF`
- Pad FX restrictions, e.g. slip loop pad FX unavailable on PHONO/LINE input. string `Invalid Operation. SLIP LOOP of PAD FX is not available, when the INPUT SELECT SW is set to PHONO/LINE.`

#### 2.9.4 Other FX families

- Slide FX (SFX) unit: ECHO, REVERB, SPIRAL with deck and sampler assign (shown for some hardware layouts). `RekordboxDJSlideFxUnitContainer`, `SlideFxEcho`, `SlideFxReverb`, `SlideFxSpiral`, `SimpleSlideFx`, strings `Slide FX assign (Deck):`, `Show SFX panel.`
- Touch FX (hold/on for units 1, 2 and master; Beat FX assignment table). `TouchFxSpecList`, `TouchFxBeatFxAsign` (SettingIF), MIDI `TouchFxHold1`, `TouchFxOnMaster`
- Stem (part) FX: apply the active Beat FX only to selected stems (Vocal, Inst, Bass, Drums). `StretchablePlayerCore::reqSetPartFxState`, `effectGui::PartFxPanel`, MIDI `FXPartSelectVocalOn` etc.
- Merge FX: two units (MFX1 for decks 1/3, MFX2 for decks 2/4) that run a timed build-up (launch FX plus sample sound), then a drop/release with release FX, sample sound volume, auto gain, presets with launch FX, release FX and two sound FX slots, cancel/abort; dongle or plan gated. `MergeFX::MFXController` (startBuildEffect, startReleaseEffect, startReleaseAndDropTimer, adjustGainAutomatically), `MergeFX::MFXBuilder`, `MergeFX::MFXSequencer`, `effectGui::MFXPreset` (getLaunchFX, getReleaseFX, getSoundFX1, getSoundFX2), `RekordboxMergeFxUnitContainer`, `RekordboxDJMergeFxLayer`, `MergeFxSamplerChannelGraph`, `DjMixerUnit::adjustMergeFxSampleSoundGain`, MIDI `MFX1`, `MFX1Abort`, `MFXSampleVolume`, `MergeFxEnabled`, `MergeFXDongleConnected` (SettingIF)
- Merge FX preset vocabulary (build FX: ECHO DROP, FILTER ECHO, JET ECHO, SPACE UP, SPACE DOWN, ROLL UP, ZIP DOWN, LONG DOWN ECHO, BRAKE DOWN ECHO, MOBIUS UP SAW, MOBIUS LFO UP SAW, MOBIUS DOWN SAW, REPEAT SNARE8/9, REPEAT UP SNARE8/9, REPEAT CLAP, REPEAT CYMBAL, REPEAT SAMPLE, REPEAT UP SAMPLE; release: ECHO OUT, UP ECHO OUT, DOWN ECHO OUT, REVERB OUT, VINYL BRAKE ECHO; sounds: CRASH CYMBAL, BOOM DOWN LIFTER1/2). Category assignment inferred from string ordering. `strings.txt` block after `[ RELEASE FX ] SPIN ECHO`
- Merge FX sample sounds: 18 WAV files `MergeFX Sample Sound 101..104, 201..209, 301..305.wav`. `Resources/MERGE FX.zip`
- Mic FX (selectable effect on mic channel, level). `MicFxSelector`, `MicEffectType`, `MicEffectLevel` (SettingIF)
- Groove Circuit FX unit and Drum release FX (see 2.10). `djplay::GrooveCircuit::FxUnit`, `startRFX`, MIDI `DrumReleaseFx`
- Channel FX, aux FX and master FX insertion points in the graph. `DjSystem::setChannelFx/setAuxFx/setMasterFx/setMergeFx`

### 2.10 Sampler, sequencer, Groove Circuit, Drum Swap

- Sampler: 16 slots in banks (bank up/down), one-shot and loop modes, per-slot gain, BPM, edit start point, double/half, colours, velocity sampler with velocity curve, sampler volume, gain, sync to master, sampler as sync master, quantise, cue, tap BPM, sampler output to master or channel. `SamplerControlPanel`, `rbxfrm::SequenceableSamplerUnit`, `rbxfrm::MultiTrackPlayerUnit`, `rbxfrm::ClipTrackPlayerUnit` (launchClip, stopClip, setGain, setPlayMode, setSyncMode, enableMasterTempo), `rbxfrm::AudioClipPlayer::applyVelocityLv`, `ClipPlayMode`, `SamplerSlot16`, `VelocityCurve` (SettingIF), MIDI `Sampler1..16 Play/Stop`, `SamplerSync`, `SamplerMaster`, `SamplerQuantize`, `VelocitySampler*`
- Clip launch with quantise and slot switching with quantise. `ClipSynchronizer::doLaunchWithQuantize`, `doSwitchSlotWithQuantize`
- Hot cue / sampler combined pad mode. MIDI `HotCueSampler`
- Sampler save directory and sampler packs (XML pack format, `mode` attribute 0 = one-shot, 1 = loop inferred). `SamplerPropertiesFile`, `SamplerSaveDirectory` (SettingIF), `OSC_SAMPLER.zip/sampler_pack.xml`
- Bundled OSC Sampler pack: NOISE, SINEWAVE, SIREN, HORN one-shots. `Resources/OSC_SAMPLER.zip`
- Step/realtime sequencer: record, play, stop, erase and mute modes, sequence length, time signature, quantise, save to slot, 8 sequence call pads, sequence load (with auto play), recording the sequencer output to audio. `rbxfrm::MidiSequencerUnit` (reqSetRecMode, reqReserveCallSequence), `MidiSequenceEditor`, `SequenceSlotManager`, `SequenceFile`, `MidiSequencerAudioRecorderUnit`, `ThreadedFixedTimeAudioWriter`, `SequencerComponent` (SequenceLengthStrings, doEraseMode, doMuteMode), `SequenceLoadAutoPlay`, `SequencerQuantize` (SettingIF), MIDI `SequencerRec`, `SequencerStart`, `SaveSequence`, `SequenceCall`
- Groove Circuit: a drum-loop multi-track player that replaces or augments the drums stem of the active deck; single or multi mode, banks, slot play, gain, capture, release FX with beat value; factory samples. `djplay::GrooveCircuit`, `djplay::GrooveCircuitView`, `DjSystem::getGCMultiTrackPlayer`, `enableGCFXThru`, `AudioGraphNodes::addGCSamplerNode`, `GrooveCircuitEnable`, `GrooveCircuitSamplesSaveDirectory` (SettingIF)
- Groove Circuit factory samples: 8 loops with BPM (House1 123, House2 125, Techno1 136, Breaks1 130, House3 140, House4 150, Breaks2 140, Breaks3 121), with pre-made ANLZ analysis files. `Resources/GROOVE_CIRCUIT_FACTORY_SAMPLES.spp` (stored ZIP containing `sampler_pack.xml`, `4-Floor Breaks Kit/*.wav`, `Anlz/0000n.DAT`)
- Drum Swap: 4 slots, single/multi mode, banks A/B, gain (max +6 dB), slot load lock, sync to deck; Drum Capture (capture the drums stem of a deck to a slot, length selectable, save directory, choice of stem). `djplay::DrumSwapSlot` (SAMPLER_TRACK_GAIN_MAX6dB, doCapture, setCaptureLength), `djplay::DrumCaptureManager`, `DrumCaptureSaveDirectory`, `DrumCaptureStem`, `DrumSwapSlotLoadLock` (SettingIF), MIDI `DrumSwap*`, `DrumSwapCapture*`
- Drum Roll and Drum Trans on the drums stem at 1/16 .. 8 (roll) and 1/16 .. 16 (trans) beats. MIDI `DrumRoll1_16` .. `DrumRoll8`, `DrumTrans1_16` .. `DrumTrans16`

### 2.11 Stems (track separation) playback

- 4-part model (VOCAL, INST, BASS, DRUMS) with a 3/4 stem count option; per-stem mute, solo (active stem mute/solo), per-stem ISO level, per-stem FX enable, stem waveform. `rbxfrm::audiopart::PART`, `DjSystem::setNumPartsForTrackSeparation`, `DjPlayerUnit::setNumStems/setPartMuteState/setPartIsoValue/setPartFxState`, `IStereoAudioSource::getStemAudioSource/setStemMuteState/setStemVolume/setStemEffectWorkState`, `pad/Stems.pad.csv` (VOCAL, INST, BASS, DRUMS, FX VOCAL .. FX DRUMS, M/S), `ActivePartModeMute`, `NumStemsPart`, `4StemsEnable` (SettingIF)
- Stems are generated on demand per deck (async separation job ahead of the playhead, "track before STEMS analysis is played" fallback to the original mix), cached on disk with memory limits; streaming tracks excluded; 180 minute limit. `audiopart::AudioPartSeparatorManager::asyncRequestSeparate/registerDeck`, `AudioPartDataManager`, `MultiPagingAudioSource::isReadyOriginStem/isReadyAllStems`, strings `The STEMS function is unavailable for tracks with a time over 180 minutes.`, `The track before STEMS analysis is played since STEMS analysis is not completed.`
- Separation engines (owned by analysis subsystem): Demucs3 and Spleeter dylibs downloaded as "STEMS Engine". `Frameworks/libDemucs3.dylib`, `libSpleeter.dylib`, strings `https://dl.rekordbox.com/application/models/demucs3/`
- Stem cues (cues stored per stem state). `rb::app::service::stem_cue::StemCue`, `StemCueEntity`

### 2.12 Recording

- Mix recording of the master (or rec out, or an external rec input on hardware) with start trigger (REC button or audio signal over threshold), stop trigger, silence level threshold, auto stop after silence (20 s per string), auto split tracks on silence, auto normalise, auto import, tag entry, disk space checks, back recording. `rbxfrm::AudioRecorderUnit` (startRecording(juce::AudioFormat*, RecInfo), AutoTrackSplitJudge, setSilenceLevelThreshold, setAutoRecStopSilenceDurationThreshold, getBytesThresholdInStorageCapacity, setBackRecording, startNoiseLevelCalibration), `GainAdjustableRecorderUnit`, `RbxThreadedWriter`, `RecStartTrigger`, `RecStopTrigger`, `RecSilenceLevelThreshold`, `RecSilenceDurationThreshold`, `RecSourceType`, `RecInputChannel`, `RecordSaveDirectory` (SettingIF), strings `Audio Signal (over -60dB)`, ` - To end recording (To detect 20 sec. of silence).`, `Automatically optimize audio level of a recorded file.`
- Rec level adjust and meter. strings `Adjust recording level.`, `Increase REC level`, `GainAdjusterUnit`
- Recording disabled when streaming or Pulselocker tracks are loaded. string `Invalid Operation. REC is disabled when Streaming tracks are loaded on a DECK.`
- Mic mix to rec out. string `Mix the MIC audio to REC OUT`
- Export loop as WAV (44.1 kHz). strings `Export Loop As WAV`, `44.1kHz`, `LoopAsWavSampleRate` (SettingIF)

### 2.13 Automix

- Automix from a playlist: start/stop, random, repeat, remove played tracks, double-click load, save as playlist, crossfade timing mode and crossfade rate, trigger before end (ms), fade-in and fade-out regions, intro/outro regions from track structure, automatic tempo control, startability checks (playlist, play status, mixer mode, Ableton Link). `djplay::AutoMixIF`, `AutoMixUnit`, `AutoMixPlayer` (getIntroRegionMs, getOutroRegionMs, addFadeinRegion, setAutoTempoControlEnabled), `AutoMixCrossfader::calculateCrossfadeLengthMs`, `AutoMixTempoController`, `AutoMixLoaderWrapper::RandomTable`, `AutoMixTrackStructure`, `AutoMixCrossfadeTiming`, `AutoMixCrossfadeRate`, `AutoMixBeforeTriggerEndTimeMs`, `AutoMixRandomMode`, `AutoMixRepeatMode`, `AutoMixRemovePlayedTrack` (SettingIF), MIDI `AutoMixStartStop`
- Automix drives the real crossfader (assign, curve, value) and warns about fader assign changes. `AutoMixCrossfaderWrapper::setCrossfaderValue/setCrossfaderAssign/setCrossfaderCurve`, `kAutomixOperateCautionFaderAssignChange`

### 2.14 Decoding and input formats

- MP3 via bundled libmpg123 (dynamically loaded), with own frame analyser, Xing/VBRI handling and accurate seek. `rbxfrm::FileReadMp3`, `Mpg123DynamicLibraryHandle`, `checkXingVbri`, `MacOS/libmpg123.0.dylib`
- AAC/M4A/MP4 via own atom parser and Apple AudioConverter; ALAC via the same path; fragmented MP4 for streaming. `rbxfrm::FileReadMp4`, `FileReadAac`, `FileReadAlac`, `Mp4AtomParser::getALACSpecificInfo`, `FileReadFMP4`, imports `_AudioConverterNew`, `_AudioConverterFillComplexBuffer`
- FLAC (JUCE embedded libFLAC), WAV, AIFF (own readers plus JUCE), Ogg Vorbis (Spotify path). `rbxfrm::FileReadFlac`, `FileReadWav`, `FileReadAiff`, `FileReadOgg`, `juce::FlacReader`, `juce::WavAudioFormatReader`, `juce::AiffAudioFormatReader`
- Fallback via JUCE/CoreAudio and ExtAudioFile/AudioFile APIs. `rbxfrm::JuceAudioReaderWrapper`, `juce::CoreAudioReader`, imports `_ExtAudioFileRead`, `_AudioFileOpenWithCallbacks`
- File delay compensation (encoder delay / priming samples) per reader. `AbstractReader::getFileDelayTimeSec`
- Bit depth detection. `rbxfrm::BitDetectX::detect`

---

## 3. Signal flow and threading (as observed)

### 3.1 Per-deck signal path (inferred ordering from class relationships)

```
file/stream bytes
  -> AbstractReader subclass (FileReadMp3/Mp4/Flac/Wav/Aiff/Ogg, StreamingReader, AudioPartReader for stems)
  -> SampleRateConverter (to engine rate)                         [reader thread]
  -> PcmReaderTemplate ring buffer / PageBuffer pages              [buffering thread]
  -> PagingAudioSource / MultiPagingAudioSource (stems: one child per part)
       warp points (loops, jumps), cue buffers, buffered ranges
  -> StretchableAudioSource
  -> StretchablePlayerCore: TimeStretchManager (FGPR stretch thread look-ahead, simple
       cross-fade fallback, scratch and scan paths), key shift, SRC speed change,
       per-stem gain (partIsoCurve), per-stem mute, Attenuator fades
  -> DjPlayerCore behaviours (cue, loop, slip, slicer, censor, sync, DVS) mutate
       position and speed in pre-render tasks before each block
  -> DjPlayerMuteUnit, AutoScratch/AutoMute (jog cutter), part FX tap
  -> RekordboxDjPlayerUnit (or Input Thru from hardware input, DVS control input)
  -> DjMixerUnit InputChannelGraph[ch]: Trim/AutoGain -> EqIso (EQ or Isolator or STEM ISO)
       -> channel FX (Color FX) -> pre-fader cue tap -> ChannelFader (curve)
       -> CrossFaderAssign/CrossFaderVolumeControl -> level meters
  -> OutputChannelGraph: sum -> master FX (Beat FX assigned to master, Release FX)
       -> master volume -> master limiter -> Master Out, Master Out 2
       -> booth (limiter) ; rec out (delay comp) ; link out ; send/return FX
  -> FixedFsAudioProcessor SRC (engine rate <-> device rate)
  -> rb::AudioDeviceManager -> CoreAudio IOProc (possibly via AudioIODeviceCombiner)
```

Beat FX units sit on a channel, the sampler or master depending on assignment (`DjSystem::setChannelFx/setMasterFx`, `RekordboxSingleFxUnitContainer::enableFxToChannel`). The exact insertion point for channel-assigned Beat FX (pre or post fader) is an open question.

Other sources entering the mixer: sampler (`SamplerInputChannelGraph`, with thru, FX and cue inputs), Merge FX sampler (`MergeFxSamplerChannelGraph`), Groove Circuit multi-track player, metronome (`getChannelIndexOfMetronomeInput`), Ableton Link metronome, preview player, mic (`MicChannelGraph`).

### 3.2 Graph construction

- The engine is a `juce::AudioProcessorGraph` populated with nodes: players, mixer, sampler, GC sampler, MFX sampler, metronome, Ableton Link, link monitor, recorder, sequencer recorder, send/return FX, auto-BPM nodes, input and output nodes. `rbxfrm::AudioGraphNodes::add*Node`, `DjUnitAudioGraph::add*`
- `DjSystem::changeActiveAudioGraph` switches between the DJ graph and the Edit-mode graph (`editfrw::EditAudioGraph`), and `reConnect` rebuilds connections when routing changes (`RouteChangeNotifier`).
- `rbxfrm::AudioProcessorParallelizer` runs selected processors (inferred: decks) as parallel jobs within one callback (`ProcessorJob::runJob`, `setNumChannelsPerProcessor`).
- `rbxfrm::ClientProcessorCaller` and `MixerComponentProcessor`/`FxUnitProcessor` wrap non-JUCE units as processors.

### 3.3 Threads

| Thread | Evidence | Role |
| --- | --- | --- |
| CoreAudio IOProc (real-time) | `_AudioDeviceCreateIOProcID`, `rbxfrm::SystemThreadIdentifier::isAudioThread`, `AudioThreadIDUpdater` | Runs the graph per block; executes queued pre-command, pre-render and post-render tasks. |
| Parallel DSP workers | `AudioProcessorParallelizer`, `getWorkgroup` (macOS audio workgroup), `_pthread_set_qos_class_self_np`, `_thread_policy_set` | Process decks in parallel inside the callback deadline (inferred). |
| Buffering threads | `PagingAudioSource::BufferingThread`, `PcmReaderTemplate::run`, `PageBufferedInputStream::run`, `BufferingStrategy::sortPageReadSequence` | Decode and fill pages ahead of and around the playhead and cue points. |
| Stretch look-ahead thread | `TimeStretchFGPR::run`, `stopThread`, `initRealMT` | Produces high-quality stretched audio ahead of time; the audio thread falls back to the simple path when it runs dry. |
| Stem separation jobs | `AudioPartSeparatorManager`, `AudioPartSeparatorJob::runJob`, `s_aboveWait`, `s_belowWait` | Separate stems ahead of playback. |
| Analysis task queue | `AudioAnalysisManager::TaskQueue`, `AnalyzedDataRequest` | Prepared data (grid, cues) for loaded tracks. |
| Recorder thread | `AudioRecorderUnit::RecThread`, `REC Thread`, `RbxThreadedWriter::useTimeSlice` | Writes recorded audio off the audio thread. |
| Async deleter | `NonBlockTaskAsyncDeleter::deleteAsync`, `IStereoAudioSource::deleteAsync` | Frees sources and buffers off the audio thread. |
| Message thread | `MessageThreadExecutor::post`, `AudioThreadCallbacker::timerCallback`, `StatBroadcaster::notifyPooledInfos` | Receives state changes from the audio thread and notifies UI listeners. |
| High-resolution timer | `HighResolutionTimerThread`, `PSvLinkTimeBeatCounter Thread` | Timing for Pro DJ Link beat broadcast and controllers (inferred). |

### 3.4 Command and state model

- Every control call has a `reqX` (callable from any thread, enqueues) and a `doX` (executed on the audio thread at a defined point). Seen across `StretchBehavior::reqPlay/doPlay`, `CueBehaveDispatcher::reqReloop/doReloop`, `MidiSequencerUnit::reqSetRecMode/doSetRecMode`.
- Queues are lock-free fixed-block message buffers with typed parameters. `rbxfrm::FastMessageBuffer` (writeMessageWith1Param .. 4Params), `MultiWriterMsgBuffer`, `OwnedFifo`, `common::PoolMessageQueue`, `PoolMessageQueueEmitter`.
- Tasks are scheduled at render phases: pre-command, pre-render, post-render, on-warped (when the reader crosses a warp point), on-fade-out-completed, on-play-position-jumped. `RecievableAudioRenderer::addPreCommandRoutineTask/addPreRenderRoutineTask/addPostRenderRoutineTask`, `AudioPlayerCore::setTaskOnWarped`, `setTaskOnFadeoutCompleted`.
- State is published by broadcasters that pool changes and flush on the message thread. `DjPlayerUnitStatBroadcaster::flush`, `StatBroadcaster::regNotifyInfo`.
- Read/write locks that never block the audio thread. `rbxfrm::NonBlockReadWriteLock`, `EasyRWLock`, `PageBuffer::lockBufferedRange`.
- Control arbitration: each actor identifies itself as an `IControlClient` (UI player, DVS, AutoScratch, controller, Pad FX view), and some actions are ignored if another client owns the deck (`mutePlayerIfNoOneControls`).

### 3.5 Buffering

- Decoded audio is held in pages (`PageBuffer::Page` with ref counts, `Float4PageBuffer`), ranges tracked by `BufferedRangeManager`; UI shows buffered ranges.
- Cue buffers (forward and reverse) keep hot cue regions resident.
- Stems use one paging source per part plus the original mix (`MultiPagingAudioSource::isReadyOriginStem`).
- Streaming sources expose `getBufferedRate` and fill progressively.

---

## 4. Internal architecture

### 4.1 Layering

1. **Application/UI layer** (`djplay::UiPlayer`, `djplay::UiMixer`, `effectGui::*`, `djplay::PadFxView`, `SamplerControlPanel`, `djplay::MixPointLinkComp`, `djplay::AutoMixIF`): turns UI, MIDI and HID events into engine calls.
2. **Facade** `djengine::DjEngineIF` (135 members: play, pause, callCue, setTempo, setEqualizer, setCrossFader, enableSlip, instantDouble, getBeatGrid, isSync, ...): the single entry point the UI uses, wrapping `DjSystem`.
3. **System** `DjSystem` / `RekordboxDjSystem` / `FourDeckSystem`: owns the `DjUnitAudioGraph`, routing manager, sync manager, key sync manager, Ableton Link, recorder, sampler, mixer control and state interfaces (`getPlayerControl`, `getMixerControl`, `getRecordingControl`).
4. **Units** (`IAudioProcessUnit` implementers): `DjPlayerUnit`, `DjMixerUnit`, `SequenceableSamplerUnit`, `MultiTrackPlayerUnit`, `AbletonLinkUnit`, `AudioRecorderUnit`, `MetronomeUnit`, `AutoBPMDetectorUnit`, `BPMManager`, `ChannelRoutingGroup`.
5. **Player core and behaviours**: `AudioPlayerCore` -> `StretchablePlayerCore` -> `DjPlayerCore`, with pluggable behaviours injected by `DjPlayerConfig::create*` (strategy pattern).
6. **Sources and readers**: `IStereoAudioSource` (Paging, MultiPaging, ChildPaging, MultiSlot, Stretchable), `AbstractReader`/`FileReader` subclasses, `AudioReaderFactory`.
7. **DSP**: `rbxfrm::dsp` (FGPR stretch, cross-fade stretch, FFT, SRC core), `rbxfrm::f2dsp` (stretch manager, Kaiser SRC, attenuator, ring buffers), effect classes (`EffectBase` subclasses and `*Core` kernels), mixer DSP (`Equalizer`, `Isolator`, `CrossFaderVolumeControl`, `TalkOver`, `MicFeedbackEliminator`, `IirFilter` family).

### 4.2 Deck composition (`DjPlayerConfig` factory methods)

`DjPlayerUnit` owns a `DjPlayerCore` plus: `StretchBehavior`, `CueBehaveDispatcher` (switching between `QuantizedCueBehavior`, non-quantised `CueBehavior`, `DummyCueBehavior`), `ActiveLoopBehavior`, `AutoCueBehavior`, `BeatSearchBehavior`, `ScanBehavior`, `SlipBehavior` (with `SlipBackGroundPlayer`), `SlicerBehavior`, `ActiveCensorBehavior`, `DjSyncSlaveBehaveDispatcher` (Beat/Bpm/DvsBpm/OutOfRange/Unsyncable), `DjSyncMasterBehavior`, `DjKeySyncMasterBehavior`, `DjKeySyncSlaveBehaveDispatcher`, `DVSBehaviorWrapper` (`DVSBehavior` + `PhaseBehavior`), `DjPlayerMixPointLink`, `OutputWaitState`, `DjPlayerQuantizePolicy`, `AutoGainAdjuster`, `PlayHistoryRegisterer`, `DjPlayerUnitSourceLoader` (loads source, receives analysis data, sets hot cue slots), `DjPlayerUnitStatBroadcaster`, and optional `ControllerPerformanceBehavior` (hardware jog processing via `DjController::*Processor`).

### 4.3 Sync model

- `SyncManager` holds masters and slaves; `LocalSyncManager` nests a local sync domain (inferred: sampler/sequencer clock group) inside the global one.
- Masters expose a playhead (`getPlayHead`, `getPlayHeadEvenIfNotHaveBeatGrid`) broadcast each block (`broadcastSyncMasterPlayHead`); slaves receive it (`setSyncMasterPlayHead`) and adjust speed (`adjustXSpeedToSync*`) and phase (`calcPhaseDiffPpq`) in PPQ units (`UnitOfTime::PPQ_8TH_NOTE`, `PPQ_32ND_NOTE`, `PPQ_128TH_NOTE`).
- Clock generators (`SimpleClockGenerator`, `FlexibleClockGenerator`) act as internal masters when no deck is master, and support loops (`exitFromLoop`, `getNumBeatsInLoop`).
- Key sync mirrors this with `KeySyncManager`, `IKeySyncMaster`, `IKeySyncSlave`.

### 4.4 Effects model

- Container per unit type (`Rekordbox*FxUnitContainer`) -> one or more layers (`RekordboxDJ*FxLayer`, `EffectLayerBase`) -> `EffectBase` instances created by `createEffectBase` -> DSP `*Core` kernels and shared building blocks (`DelayBufferBase`, `StereoRingBuffer`, `MonoRingBuffer`, `CrossFadeDelayBuffer`, `SmoothFloat`, `GainSmoother`, `IirLpf2ndOrder`, `IirHpf2ndOrder`, `IirBpf2ndOrder`, `EffectFrequencyModule`, `CompCore`, `GateCore`).
- Beat-synchronised effects get BPM and playhead (`EffectBase::setBPM`, `playHead`, `setTimeUntilNextPlayHead`) and quantised start via `FxQuantizeManager`.
- `EffectBase::processSubsonicHPF` suggests a subsonic high-pass on effect output (inferred).

### 4.5 Device I/O

- `rbxfrm::SingletonAudioDeviceManager` owns `rb::AudioDeviceManager` (JUCE-derived) which opens a `CoreAudioIODevice` or an `AudioIODeviceCombiner` (separate input/output devices).
- `rbxfrm::AudioDeviceController::insertAudioProcessor` connects the graph to the device; `setCpuLoadLogger` and `onRegisteredRenderTime` record render timing.
- `rb::CallbackMaxSizeEnforcer` splits large device buffers into engine-sized blocks (inferred).

---

## 5. Prior art and licences

| Project | What it informs | Licence | Use in Cuepoint |
| --- | --- | --- | --- |
| Mixxx (engine, sync, effects, vinyl control) | Overall DJ engine architecture, sync lock, effect chain design, key lock via Rubber Band/SoundTouch | GPL-2.0-or-later | Read for concepts only. Cuepoint is MIT OR Apache-2.0 (`Cargo.toml`), so no code may be copied. |
| xwax | Timecode decoding design (zero crossings, LFSR position lookup, pitch from carrier) | GPL (v2 per Wikipedia and Fedora, v3 in recent README; verify `COPYING`) | Concepts only. rekordbox uses its own 1 kHz timecode, so the position code must be learned from the real control vinyl/WAV, not from xwax tables. |
| Rubber Band Library | High-quality phase vocoder time-stretch/pitch-shift | GPL-2.0-or-later or commercial | Not usable under MIT/Apache; reference for quality comparisons only. |
| Signalsmith Stretch | Modern stretch/pitch algorithm | MIT | Algorithm is permissively licensed; Rust crates `signalsmith-stretch` and `ssstretch` wrap the C++ (C++ build dependency). A pure Rust port is feasible and licence-compatible. |
| `timestretch` crate (WSOLA + phase vocoder, rustfft only), `wsola` crate | Pure Rust stretch baselines | check crate licences before use | Benchmark or starting point. |
| SoundTouch | WSOLA tempo/pitch (Mixxx's legacy key lock) | LGPL-2.1 | Concept reference for the "simple" key-lock fallback. |
| Ableton Link | Tempo/phase/start-stop session sync | GPL-2.0-or-later or proprietary licence from Ableton | No pure Rust implementation found. `rusty_link` (bindings to `abl_link`, C++ build) is GPL-2.0-or-later, incompatible with MIT/Apache distribution. Options: proprietary licence from Ableton, a clean-room Rust implementation from a protocol description, or an optional GPL-licensed plugin crate. Decision needed. |
| JUCE | Graph and device-manager patterns that rekordbox itself uses | AGPL-3.0 or commercial (JUCE 8) | Concepts only. |
| Symphonia | Pure Rust decoders: MP3, AAC-LC, ALAC, FLAC, WAV, AIFF, Vorbis, MP4/ISO-BMFF demux | MPL-2.0 (file-level copyleft, compatible as a dependency) | Primary decoder. Gaps: HE-AAC (SBR/PS) not supported; must be checked against rekordbox-supported files. |
| cpal | Cross-platform audio I/O (CoreAudio, WASAPI, ASIO feature, ALSA, JACK) | Apache-2.0 | Device layer. Aggregate device handling must be added on top. |
| rubato | Pure Rust resampling (sinc, polynomial, FFT) | MIT | Engine-rate SRC and varispeed. |
| rustfft / realfft | FFT for stretcher and spectral FX | MIT OR Apache-2.0 | Stretcher, analysis-time tools. |
| rtrb | Wait-free SPSC ring buffer | MIT OR Apache-2.0 | Command and event queues. |
| basedrop / arc-swap / triple_buffer | Real-time safe deallocation and state publication | MIT OR Apache-2.0 | Async drop and UI snapshots. |
| assert_no_alloc | Detect allocation on the audio thread in tests | BSD-2-Clause | Test-only RT safety guard. |
| hound | WAV writing | Apache-2.0 | Recording and loop export. |
| rekordbox manuals (public PDFs from rekordbox.com) | User-facing FX names, parameter meanings and ranges, mixer behaviour | Proprietary documentation | Read for behaviour descriptions, write our own text. |

`libFilSiNE_Mac_DyLib.dylib` identification: it is **not** audio. Its exports (`_ChkMountd`, `_ChkNFSd`, `_ChkPortMapperd`, `_tkfAddExportListEntry`, `_tkfCheckPermissionByIp`, `_SetFileExtensionList`, `/nfs_daemon_fs`) and the app strings `PSvLinkNFSd Thread`, `PSvLinkMntdThread`, `PSvLinkPortMapperThread` show it is an embedded NFS server (portmapper, mountd, nfsd) with an export list restricted by file extension and IP permission. CDJs mount the rekordbox library over NFS for Pro DJ Link track loading. It belongs to the Pro DJ Link subsystem.

Bundled sample content: `GROOVE_CIRCUIT_FACTORY_SAMPLES.spp` (stored ZIP, 8 drum loops plus ANLZ files and XML), `MERGE FX.zip` (18 one-shot/riser WAVs), `OSC_SAMPLER.zip` (4 one-shots plus XML), `binary/Click Sound 0[1-3]*.wav` (metronome), `pad/*.pad.csv` (pad mode layouts). These are proprietary audio assets: Cuepoint must ship its own replacement samples with matching roles (count, loop/one-shot mode, BPM metadata) and may optionally import the user's own rekordbox install at runtime. The `.spp` and `sampler_pack.xml` formats should be supported for import.

---

## 6. Open questions for Ghidra

Each item names the symbol to decompile and what it answers.

1. `rbxfrm::dsp::FGPRImpl::operate`, `FGPRImpl::analyze`, `FGPRImpl::modifyPhaseToComplex`, `dsp::FftBuffer::adaptWindowFunction`: frame size, hop, window, phase-locking strategy of the HQ key-lock stretcher.
2. `rbxfrm::dsp::TimeStretchFGPR::operateSimple`, `operateSimpleToReal`, `isExhaustedKeyLockBuffer`, `run`: look-ahead depth, when the simple fallback triggers and how the cross-fade back to HQ works.
3. `rbxfrm::dsp::SimpleCrossFade::calculateStretchData`, `dsp::CrossFadeTimeScaling::operate`: the simple key-lock algorithm (overlap length, search window).
4. `rbxfrm::f2dsp::TimeStretchManager::operate`, `StretchablePlayerCore::doSetStretchMode(f2dsp::TimeStretchMode)`, `StretchablePlayerCore::execSetMasterTempoType(int, bool)`: enum values for stretch mode and master tempo type and how they map to the dsp classes.
5. `rbxfrm::f2dsp::c_src::init_kaiser`, `initIdxAndIrForKaiser`, `convolute_fine`: SRC filter length, Kaiser beta, oversampling, used for varispeed and device SRC.
6. `rbxfrm::UnitOfTime::defaultSampleRate`, `UnitOfTime::setDefaultSampleRate`, `FixedFsAudioProcessor::prepareSRCModules`: the fixed internal engine sample rate.
7. `rbxfrm::StretchBehavior::createSemitoneConvertTable`, `convertSemitoneDiff`: semitone to ratio mapping, maximum key shift range.
8. `rbxfrm::DjSyncSlaveBehaveDispatcher::calcPhaseDiffPpq`, `changeBehaviorIfNeeded(PlayHeadInfo const*)`, `SlavePlayerFunctions::adjustXSpeedToSyncBeat`, `getMinJumpSamplesToSyncAtHead`: phase correction law (jump vs speed nudge), thresholds, when Beat Sync drops to BPM Sync or OutOfRange.
9. `rbxfrm::SyncManager::decideSyncMaster`, `shouldChangeMasterByPriority`, `ISyncMaster::getDefaultPriorityAsSyncMaster`, `getSubPriorityAsSyncMaster`: automatic master hand-over rules.
10. `rbxfrm::KeySyncManager::decideKeySyncMaster`, `isKeyMatching`, `KeySyncBehavior::getSyncKeyCode`: key-matching rule (same key, relative, Camelot neighbours) and shift direction choice.
11. `rbxfrm::QuantizedCueBehavior::thresholdForInput`, `DjPlayerQuantizePolicy::threshold`, `thresholdPpq`: how early a quantised action may fire and when it waits for the next beat.
12. `rbxfrm::VinylSpeedAdjustor::setAccelerationByDefaultTable(Direction,int)`, `PitchSmoother::createPitchTable`: default brake/start curves and table values.
13. `rbxfrm::ScanBehavior::updateScanPosition`, `f2dsp::TimeStretchScan::setRate`: scan audio rendering.
14. `rbxfrm::f2dsp::TimeStretchScratch::operate`, `readAround`, `StretchBehavior::isScratchableByInertia(float)`: scratch interpolation and inertia model.
15. `rbxfrm::DVSSignalDecoder::processDecode(...)`, `TimecodeInfo::getNextBitStream`, `getNextTimecode`, `TimecodeInfo::Templates::createTimecodeInfo(DVSControlDiscSideType)`: timecode modulation, bit encoding and position code generator for rekordbox Control Vinyl and Control CD.
16. `rbxfrm::PositionDetectLevelDiff::detect(float,bool)`, `PositionDetectPeakAverage::detect`, `DVSControlSignalState::update`: bit slicing and quality metrics.
17. `rbxfrm::DVSRelativePitch::updateStateAndApplyRelativePitch...`, `DVSEmergencyInternalMode::checkProcess`, `DVSNeedleDrop::processJumpToHead`: relative mode drift handling and signal-loss behaviour.
18. `CrossFaderVolumeControl::setCurve(ECrossFaderCurve_t,int)`, `InputChannelGraph::selectChannelFaderCurve`, `ChannelFader`, `ControllerCrossFaderCutLagComponent`: exact fader and crossfader gain laws and cut lag.
19. `Equalizer` tables `mp_hi_Gain_eqtbl`, `mp_mid_LowQ_eqtbl`, `mp_s_vr_eqtbl`; `EqualizerNXS2::mp_mid_Gain_eqtbl`; `Isolator::applyFilter`, `Isolator::mpCurveAd`: EQ crossover frequencies, Q, knob-to-dB curve, kill depth.
20. `StretchablePlayerCore::partIsoCurve`, `PartIsoControlComp::setPartIsoValue`: STEM ISO knob-to-gain curve.
21. `InputChannelGraph::getTrimHeadRoomdBFS`, `AutoGainAdjuster::calcNotClippingGain(float,float)`, `HeadRoomLevel` (SettingIF): trim range, headroom default and auto-gain target loudness.
22. `OutputChannelGraph::enableMasterLimitter`, `CompressorUnit`, `rbxfrm::Compressor::adjustParameter`: master and booth limiter thresholds and time constants.
23. `TalkOver::processMicInput`, `TALKOVER_LEVEL_TABLE`, `TALKOVER_ADV_FILTER_POS_TABLE`: talkover ducking depth, attack/release and advanced mode filter.
24. `MicFeedbackEliminator::calcFcAndGain`, `setType`, `MicFxSelector::initFxList`: feedback eliminator types and the mic FX list.
25. Every Beat FX: `RekordboxDJSingleFxLayer::createEffectBase`, `RekordboxDJMultiFxLayer::createEffectBase`, then each class's `getParameterRange`, `getParameterDefaultValue`, `getParameterNumSteps`, and `processEffect`: effect id order (the FX list order), parameter ranges and DSP. Highest-value targets: `EchoCore::processEffect`, `SpiralCore`, `ReverbCore`, `HelixCore`, `RollCore`, `TransCore`, `LFOFilterCore`, `StretchCore`, `SingleBeatFxVinylBrake`, `BeatFxPan::Trans`.
26. `RekordboxDjColorFxLayer::createEffectBase`, `ColorFxFilterDjm900NXS2::updateFilterCoef`, `ColorFxFilterManipulandumCutoffFreqConversionTable`, `ColorFxCrushFilterCutoffTable`, `ColorFx::COLOR_MANIPULANDUM_SMOOTHING_TIME_MS`: CFX order, knob-to-cutoff mapping and smoothing.
27. `djplay::SmartCFX::SMART_CFX_PRESET_LIST`, `SMART_CFX_PRESET_INDEX_LIST`: what each Smart CFX preset combines.
28. `RekordboxDjPadFxLayer::createEffectBase`, `applySpecialBeatParams`, `applySpecialCycleTimeMs`, `djplay::PadFxSettings::padFxStandardPreset`: Pad FX catalogue and default pad assignments.
29. `MergeFX::MFXPattern::initPattern`, `effectGui::MFXPreset::tablePreset/tableLaunchFX/tableReleaseFX/tableSoundFX1`, `MergeFX::MFXController::startReleaseAndDropTimer`: Merge FX presets, build length, drop timing.
30. `FxQuantizeManager::processQuantizeEffect`, `adjustPlayAHeadTiming`: how FX on/off is quantised to beats.
31. `djplay::AutoMixCrossfader::calculateCrossfadeLengthMs`, `AutoMixPlayer::addFadeinRegion/addFadeoutRegion`, `AutoMixTempoController::timerCallback`, `AutoMixLoaderWrapper::getNextRandomRow`: Automix timing modes, crossfade shape and tempo ramp.
32. `rbxfrm::DjPlayerMixPointLink::onPreRender`, `djplay::MixPointLinkComp::getRealMixPointTime`: Mix Point Link start timing and BPM sync auto change.
33. `rbxfrm::AutoScratchUnit::calcTableAndTrigger`, `ScratchTables`, `AutoMuteUnit::MuteTables`, `djplay::JogCutterManager::executeJogCutter`: Jog Cutter patterns.
34. `rbxfrm::SlicerBehavior::doSetUnitWidth`, `RollTaskOnWarped`, `IDjPlayerSlicer::Mode`: slicer domain size, roll widths, slicer vs slicer loop.
35. `rbxfrm::AudioRecorderUnit::startRecording`, `AutoTrackSplitJudge`, `checkAndCreateThreadedWriter`: recording file format, bit depth, split rule.
36. `rbxfrm::AbletonLinkUnit::updatePlayHead(int)`, `doRenderOutputAudio`: output latency compensation and quantum handling with Link.
37. `rbxfrm::AudioProcessorParallelizer::addProcessor`, `ProcessorJob::runJob`: which nodes run in parallel and how workers join the audio workgroup.
38. `rbxfrm::PagingAudioSource::BufferingThread::doBuffer`, `BufferingStrategy::sortPageReadSequence`, `ReaderOwnerTmpl::CUE_BUFFER_SIZE_FWD`: page size, read-ahead and cue buffer sizes.
39. `rbxfrm::GainAdjusterUnit::setGainRange`, `DjMixerUnit::setRecOutDelayTimeMs`: rec level range and rec out delay.
40. `djplay::DeviceDefs::s_deviceInfo`: per-device mixer mode, channel maps, mic and send/return counts (shared with the controller subsystem).

---

## 7. Rust plan

### 7.1 Crates and modules

The workspace already has `cuepoint-core`, `cuepoint-dsp`, `cuepoint-engine`, `cuepoint-control`, `cuepoint-analysis`. Proposed split:

- `cuepoint-core`: shared types used by engine and library. `Ppq`, `SamplePos`, `BeatGrid` (with bar/beat index), `KeyCode` (24 keys plus Camelot/alphanumeric display), `Fraction` (beat values like 1/64), `CuePoint`, `Loop`, `ActiveCensorRange`, `StemPart { Vocal, Inst, Bass, Drums }`, `QuantizeUnit`.
- `cuepoint-dsp` (no threads, no allocation after `prepare`, `#![no_std]`-friendly where practical):
  - `stretch`: `PhaseVocoderStretcher` (HQ, clean-room, Signalsmith-style or our own phase-locked vocoder), `OlaStretcher` (simple fallback), `ScratchResampler`, `ScanRenderer`, `KeyShift` (pitch via stretch plus resample).
  - `resample`: windowed-sinc SRC (own or `rubato`).
  - `filters`: biquads, 1st order IIR, `SmoothedParam`, `Attenuator` (fade in/out, discontinuity smoothing), DC/subsonic HPF.
  - `mixer`: `ThreeBandEq` (default and NXS2-style), `Isolator`, `FaderCurve`, `CrossfaderCurve`, `Limiter`, `TalkoverDucker`, `FeedbackEliminator`, `LevelMeter`.
  - `fx`: trait `Effect { prepare, set_param, set_bpm_and_playhead, process, reset }`, plus one module per FX family (`beat`, `color`, `pad`, `release`, `merge`, `slide`, `mic`) and shared kernels (`delay_line`, `comb`, `allpass`, `lfo`, `roll_buffer`).
  - `dvs`: timecode demodulator (carrier tracking, polarity, bit slicer, position LFSR/table lookup, quality metrics).
  - `bpm`: real-time beat detector for external input, tap tempo.
- `cuepoint-engine`:
  - `io`: `cpal` backend, device enumeration, aggregate (combined input/output) support, channel routing matrix, internal vs external mixer mode, xrun and CPU stats.
  - `graph`: static, pre-allocated processing graph (decks, sampler, mixer, FX buses, outputs) rebuilt off-thread and swapped atomically on routing change.
  - `deck`: `Deck` state machine with behaviours as plain structs (cue, loop, slip, slicer, censor, scan, vinyl speed, auto cue, mix point link), `SourceReader` (paged decode), `CueBuffers`.
  - `sync`: `SyncManager` (master election, Beat/BPM policy, slave states including OutOfRange and Unsyncable), `KeySync`, `ClockGenerator`, `link` (Ableton Link adaptor behind a feature flag).
  - `sampler`: 16-slot clip player, sequencer (MIDI-like event list), Groove Circuit, Drum Swap, Drum Capture.
  - `stems`: multi-source deck reader (original plus 4 stems), per-stem gain/mute/FX send.
  - `record`: lock-free capture ring to a writer thread, silence detection, auto split, normalise pass.
  - `automix`: track scheduler and crossfade automation (non-RT, drives engine commands).
  - `command` and `state`: `EngineCommand` enum (mirrors the req/do split), `EngineEvent`, snapshot structs for the UI.
  - `render`: offline renderer (no device, deterministic clock) used by tests and "export".
- `cuepoint-analysis` owns stem separation; the engine only reads separated stems through a `StemProvider` trait with progressive readiness.

### 7.2 Dependencies (pure Rust unless flagged)

- `cpal` (Apache-2.0): I/O. On macOS it links CoreAudio system frameworks (not a C library we ship, acceptable). ASIO on Windows needs the Steinberg SDK (flag, Windows only).
- `symphonia` (MPL-2.0): MP3, AAC-LC, ALAC, FLAC, WAV, AIFF, Vorbis, MP4 demux. **Gap: HE-AAC.** Option: optional `platform-decoder` feature using AudioToolbox via FFI on macOS (system framework, needs an `unsafe` exception crate since the workspace denies `unsafe_code`).
- `rubato` (MIT), `rustfft`/`realfft` (MIT OR Apache-2.0), `rtrb` (MIT OR Apache-2.0), `triple_buffer` (MIT), `basedrop` (MIT OR Apache-2.0), `hound` (Apache-2.0), `crossbeam-channel` (non-RT threads only), `assert_no_alloc` (dev).
- Time-stretch: implement in `cuepoint-dsp` (preferred, licence-clean). Do not depend on Rubber Band (GPL) or SoundTouch (LGPL).
- Ableton Link: **flag**. No pure Rust implementation exists; `rusty_link` needs a C++ toolchain and is GPL-2.0-or-later. Keep behind `link` feature in a separate crate until the licensing decision is made.
- Stem separation runtime (analysis subsystem): `ort` wraps the C++ ONNX Runtime (flag); `tract` (MIT OR Apache-2.0) or `candle` are pure Rust alternatives.
- Real-time thread priority and macOS audio workgroups: `audio_thread_priority` (MPL-2.0, contains platform FFI; flag for `unsafe`).

### 7.3 Real-time safety rules

1. The audio callback never allocates, frees, locks a mutex, performs I/O, logs, or calls into the OS beyond what cpal requires. Enforced in tests with `assert_no_alloc` wrapping the offline renderer.
2. All control goes through bounded SPSC queues (`rtrb`) as `EngineCommand` values that are `Copy` or carry pre-allocated handles. Commands are applied at fixed points in the block: pre-command, pre-render (behaviours update position and speed), render, post-render (mirrors rekordbox's task phases).
3. Large objects (decoded pages, new track sources, FX state, rebuilt graphs) are built on worker threads and handed over by pointer swap; old objects are returned through a "trash" queue and dropped off-thread (basedrop-style).
4. UI state is published once per block via triple buffers or atomics; the UI never reads engine internals directly.
5. Every parameter change is smoothed (per-sample or per-block ramps); every discontinuity (jump, loop wrap without warp point, source change, pause) is covered by a short cross-fade (`Attenuator` equivalent).
6. Decoding runs ahead in a buffering thread with a page cache and cue buffers; if the playhead hits an unbuffered page the deck outputs silence for that block and flags an underrun instead of blocking.
7. The HQ stretcher runs look-ahead on a worker thread; the audio thread consumes its output and falls back to the cheap OLA path if the buffer is empty, then cross-fades back (behaviour observed in `TimeStretchFGPR`). Scratch and scan always use the direct resampling path.
8. Denormals are flushed (FTZ/DAZ) at the start of each callback.
9. Optional parallel deck processing uses pre-spawned workers joined to the audio workgroup on macOS, with spin-then-park handoff and a deadline fallback to serial processing.
10. Fixed internal processing rate (to be confirmed, see open question 6) with one SRC stage to the device, so DSP coefficient tables and FX timing match rekordbox regardless of device rate.

### 7.4 Parity test strategy

Ground truth is captured from real rekordbox 7.2.19 on macOS; captures are kept outside the public repo (they contain rekordbox output and possibly copyrighted test tracks). Test material is our own synthetic signals and licence-free music.

- **Capture rig**: rekordbox output routed to a loopback device (for example BlackHole) and recorded at 32-bit float; actions are driven by a scripted virtual MIDI controller using rekordbox's own MIDI-learn command names (section 2 MIDI table) so every action is timestamped and repeatable. Each capture stores the script, rekordbox settings export and the recording.
- **Offline renderer**: `cuepoint-engine::render` replays the same script deterministically (block-accurate command timing) to a WAV.
- **Alignment and null test**: cross-correlate to remove latency offset, then subtract. Pass thresholds per feature: bit-near for pure gain paths (EQ flat, faders, trim), residual below -60 dBFS for linear filters and EQ curves, perceptual/spectral metrics for nonlinear or time-varying FX and stretch.
- **System identification for linear blocks** (EQ, isolator, CFX filter, crossfader and channel fader curves, trim, headroom): log sine sweeps and stepped DC/sine levels at a grid of knob positions; compare magnitude and phase responses to rekordbox captures within ±0.1 dB. This yields curves without decompilation.
- **FX characterisation**: impulse and burst trains at fixed BPM per effect, per beat value and per parameter step; compare delay times, feedback decay envelopes, LFO periods (beat-locked), wet/dry law, and tail behaviour after FX off and after release FX.
- **Stretch and key**: sine, chirp, drum loops and speech at tempo ratios ±6/±16/±50 % and key shifts ±12; compare with log-spectral distance and transient smearing metrics; check exact output length and position drift over 10 minutes (must be zero samples of drift relative to beat grid).
- **Sync**: two decks with known grids, sync from various phase offsets and tempos; measure beat phase error over time from onset detection, convergence time and whether a jump or a nudge occurred; repeat for BPM Sync, half/double sync rate, master hand-over.
- **Cue/loop timing**: quantised hot cue, loop in/out, beat jump, slip return; measure sample position of resulting audio against the grid; loops must be seamless (null test across the wrap).
- **DVS**: play the rekordbox Control WAV (Control CD) through a loopback into the DVS input at varying speeds, reverse, needle drops, noise; compare decoded position and pitch traces logged by our decoder against rekordbox playback captured simultaneously.
- **Latency**: measure input-to-output (DVS, input thru) and MIDI-to-audio latency on both apps with the same device and buffer size via a loopback click; Cuepoint must be no worse than rekordbox.
- **RT safety and performance**: CI runs the offline renderer for 4 decks with stems, HQ stretch, 3 Beat FX and recording under `assert_no_alloc`; benchmark worst-case block time against the 128-sample budget at 48 kHz.
- **Regression corpus**: once a feature matches, our own render becomes the golden file in the repo (only our output is committed), so later changes are caught without needing rekordbox.
