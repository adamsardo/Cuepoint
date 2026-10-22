# 06. Controllers, hardware I/O, lighting and video

Status: draft from static evidence only (rekordbox 7.2.19, macOS, arm64). No Ghidra or REA session was used. Every statement is either backed by an evidence reference in backticks, or marked **(inferred)**. Evidence paths are relative to `/opt/rekordbox/7.2.19/`: `classes.tsv`, `strings.txt`, `syms.demangled.txt` and `imports.txt` live in `analysis/`; resources live in `payload/rekordbox/rekordbox.app/Contents/Resources/` (abbreviated `Res/`).

Scope boundaries:

- In scope: physical DJ hardware I/O (MIDI, HID, LAN HID), mapping files and MIDI Learn, controller feedback (LEDs, jog rings, on-jog and player displays), DJM mixer integration on the hardware side, keyboard shortcuts, the OSC status monitor, the hardware-facing part of Ableton Link, lighting mode (fixtures, venues, macros, DMX output) and video mode (video decks, transitions, overlays, camera, karaoke, Syphon).
- Out of scope (owned by other subsystem specs): the audio engine and `DjMixerUnit` DSP, Beat FX and Colour FX DSP, the Pro DJ Link protocol itself, Ableton Link session logic, the pad editor and performance pad behaviour (we only cover how pads arrive from hardware), phrase analysis (we consume its output), library and Inflyte (`Inflyte`, `browse::InflyteBrowser` are a promo-pool service, not lyrics).

---

## 1. Purpose

This subsystem turns hardware gestures into rekordbox commands and turns application state back into hardware feedback, and it drives two non-audio outputs: DMX lighting and a video mixer. For parity, Cuepoint must:

1. Recognise every controller, mixer and player rekordbox 7.2.19 recognises, map its controls to the same functions, and drive the same feedback (LEDs, jog rings, displays).
2. Offer MIDI Learn with the same function vocabulary, and import a user's rekordbox `.midi.csv` mapping files.
3. Offer configurable keyboard shortcuts with the same command set and presets.
4. Provide lighting mode: fixture library, venues, phrase-driven macro scenes, a scene editor, live performance controls and DMX output through the same interface classes.
5. Provide video mode: two video decks, transitions, video FX, text/image/camera overlays, CDG karaoke, an output window and Syphon output.

---

## 2. Feature inventory and parity checklist

Each bullet is a parity checklist item. `[ ]` means not yet implemented in Cuepoint.

### 2.1 Device discovery and transport

- [ ] Enumerate CoreMIDI sources and destinations, open ports, receive and send MIDI, including MIDI 2.0 event-list APIs `imports.txt: _MIDIClientCreate _MIDIInputPortCreateWithProtocol _MIDISendEventList _MIDIReceivedEventList`
- [ ] Hot-plug detection of MIDI and HID devices with periodic checks `djplay::DeviceServer: MIDI_DeviceCheck MIDI_DeviceCheck2 HID_DeviceCheck EXP_DeviceCheck`, `DeviceChangeDetectorEx`
- [ ] HID devices through IOHIDManager: matching, input report callback, set report `imports.txt: _IOHIDManagerCreate _IOHIDDeviceRegisterInputReportCallback _IOHIDDeviceSetReport`, `djplay::HidManager`, `djplay::MAC_HidInput`, `HidRunLoopClass`
- [ ] HID over LAN for networked players (CDJ/XDJ in "HID over Ethernet" mode) **(inferred from name)** `djplay::HidMapLANHID`, `DeviceManager::connectLanHidDevice`
- [ ] USB descriptor probing (bcdDevice, bcdHID, serial number) for device validation `djplay::UsbProbe: getBcdDevice getBcdHid getSerialNumber`, `djplay::USBDeviceValidation::getBcdVersion`
- [ ] Device name normalisation and aliasing ("PIONEER " prefixes, " MIDI" second ports, "2IN2OUT" variants, "(1)"/"(2)" multi-port devices) `DeviceManager: isAliasName getRealNameFromAlias`, alias table in `strings.txt` near `DJM-nexus-900`, `ALL-2RX`, `2IN2OUTSX2`
- [ ] Per-device capability table (controller/mixer/player class, pad count, mic count, FX banks, send/return, motorised jog, mixer mode, stems buttons, MIDI vs HID, HID-after-MIDI-auth) `djplay::DeviceDefs` (147 members: `getPadNum isMotorizedJogDevice getNumMics getNumFxBanks getStemsLayout isHidOpenAfterMidiAuth isPioneerDongleDevice getThirdpartyInfo s_deviceInfo`)
- [ ] Third-party device groups and product IDs (Phase) `DeviceDefs: getThirdpartyGroupName getThirdpartyProductID isThirdpartyHidDevice isThirdpartyMidiDevice`, `strings.txt: Phase Essential, Phase Pro, Phase Pro Receiver`
- [ ] Bluetooth LE MIDI pairing window (macOS CoreAudioKit panel) `imports.txt: _OBJC_CLASS_$_CABTLEMIDIWindowController`
- [ ] "Not supported device" warning, firmware update manager link, Setting Utility install/update prompts `NotSupportedDeviceWindow`, `SettingUtilityUpdateWindow`, `strings.txt: firmware-update-manager, If you want to use xxxxx as an audio interface, you need to install the Setting Utility.`
- [ ] Device usage logging `DJDeviceUsageLogManager`
- [ ] Product information, firmware version and serial number requests over SysEx/HID `djplay::MidiSysExVer`, `MidiSysExSer`, `MidiSysExRec`, `MidiMap: requestVersion requestSerialNumber`, `strings.txt: @ProductInfoReq @ProductInformationRequest @SerialNumber @Version`
- [ ] Hardware Unlock detection (devices that unlock paid features) `DeviceManager: isDongleAvailable undetectDongleDevice setAllowNonDongleDevices`, `strings.txt: Hardware Unlock device`. **Not to be reimplemented** as a licence check; see section 6.
- [ ] MIDI output on a background thread with priority ordering and a configurable output interval `djplay::MidiOutputHelper::sendPriorityMessage`, `djplay::PriorityMidiMessage`, `djplay::MidiOutputBackgroundThread`, CSV `MidiOutInterval` (`FFF3`)
- [ ] HID output helper with priority and buffer counting `djplay::PdjUsbHidOutputHelper: getPriority getBufferCount sendMessage`

### 2.2 Built-in mappings and control semantics

- [ ] Load built-in mapping CSVs by device name (`<name>.midi.csv` under `MidiMappings`) `strings.txt: .midi.csv, MidiMappings, @file,%d,`, `Res/MidiMappings/*.midi.csv` (69 files)
- [ ] Per-device code hooks layered over the CSV (custom LED defaults, deck host mapping, pad behaviours, beat/BPM/effects info SysEx) `djplay::MidiMap*` (53 subclasses, for example `MidiMapDDJFLX10: makeBeatInfo makeEffectsInfo getBpmInfoMessage getLeftHostDeck getRightHostDeck`)
- [ ] Control element types: Button, Pad, Toggle, KnobSlider (7-bit), KnobSliderHiRes (14-bit), Slider, Rotary, RotaryAsKnob, Difference, Value, RawValue, Parameter, Indicator, IndicatorHiRes, JogRotate, JogRotateAbsPos, JogTimestamp, JogTouch, JogIndicator, JogIndicatorHiRes, SysEx, variable MSB/LSB pairs `djplay::MidiButton MidiPad MidiToggle MidiKnobSlider MidiRotary MidiRotaryAsKnob MidiDifference MidiValue MidiRawValue MidiParameter MidiIndicator MidiIndicatorLong MidiJogRotate MidiJogRotateAbsolutePosition MidiJogRotateTimeStamp MidiJogTouch MidiJogIndicator MidiJogIndicatorLong MidiSysEx MidiVariableMsb MidiVariableLsb MidiSx2TrimVariableLsb`
- [ ] Legacy per-model pad classes (DDJ-RX, RZ, SX, SX2, SZ) `djplay::MidiDdjRxPad MidiDdjRzPad MidiDdjSxPad MidiDdjSx2Pad MidiDdjSzPad`, `strings.txt: DdjRzPad DdjRxPad DdjSzPad DdjSx2Pad DdjSxPad`
- [ ] Shift layer, long press, double click (for example Load double click = Instant Double) `MidiMap::DoubleClick`, CSV modifiers `+Shift +LongPress +Press`, comment "Load to Deck / Instant Double (double click)" in `DDJ-FLX4.midi.csv`
- [ ] LED blink with per-control period `djplay::BlinkManager`, `BlinkCallback`, CSV option `Blink=250|500|600|1000`
- [ ] LED initial and default values on connect `MidiMap: forEachOutputLedDefaultValue forEachOutputLedInitialValue getLedDefaultValue`, `DeviceManager::initLedStatus`
- [ ] Soft takeover for absolute controls (and separately for BPM, EQ/part isolator) `ControllerTakeOver`, `strings.txt: TakeOver TakeOverBpm TakeoverEqPartIso`, `MidiMapOMNISDUO::TakeOver`, `DeviceConnector::isTakeOverEnabled`
- [ ] Dual deck control (decks 1+3 and 2+4 driven together) `MidiMap::getMidiInputDataDual`, `MidiLearnComponent::setDualDeck`, CSV `DualDeckState13 DualDeckState24`, option `Dual`
- [ ] Deck select / deck 3-4 layers per controller side `MidiMap: getLeftHostDeck getRightHostDeck`, CSV `Deck3 Deck4 DeckState`
- [ ] Device-pushed state notifications (play status, beat, BPM, pad mode, effects info) `strings.txt: @PlayStatusNotify @BeatNotifyDJM @BeatInfo @BpmInfo @PadModeInfo @FX1EffectsInfo @FX2EffectsInfo`
- [ ] Device parameters written at init (jog indicator interval, jog latency, MIDI out interval) CSV rows `JogIndicatorInterval FFF1 Value=12`, `JogRatency FFF2 Value=4`, `MidiOutInterval FFF3 Value=2`
- [ ] Hardware MySettings pushed over MIDI (jog brightness, LCD/OLED brightness, demo mode, auto standby, cross fader curve/cut lag/reverse, fader start, mic settings, peak limiters, master/booth attenuation and mono/stereo, headphone split/mono, back spin length, slip flashing) CSV names ending `Setting` (for example `JogBrightnessSetting LcdBrightnessSetting OledBrightnessSetting DemoModeSetting AutoStandbySetting CrossFaderCutLagSetting MicLowCutFilterSetting MasterPeakLimiterSetting BoothAttLevelSetting HeadphoneMonoSplitSetting BackSpinLengthSetting SlipModeFlashingSetting`), preference components `Controller*` (99 classes, for example `ControllerDisplayBrightness ControllerCrossFaderCutLagComponent ControllerMicLowCutFilterComponent`)
- [ ] Hardware DSP Beat FX hand-off on DJMs with built-in FX (FX applied in hardware vs rekordbox) `effectGui::HardwareDSPBeatFXState`, `strings.txt: @HardwareDSPBFX, BeatFX : rekordbox ==> H/W DSP`
- [ ] Merge FX and drum swap/roll hardware controls (DDJ-FLX10, FLX6, GRV6, REV5) CSV `MFX1 MFX2 MFX1Illumi.PRESET1-4 DrumSwap1-4 DrumRoll* DrumTrans*`, `MergeFX::MFXController`
- [ ] Stems/part controls (active part, part isolator, FX part select, instant doubles per part) CSV `ActivePart* PartIsolator* FXPartSelect* *PartInstDoubles`
- [ ] Touch screen strips/areas on DDJ-RZX (X/Y positions) CSV `TouchScreen.Left.X ... TouchScreen.Right.Y` (type `Slider`)
- [ ] Video controls on XDJ-RX2 and XDJ-XZ (camera, image, text, slideshow, transition FX index/favourites/random, AV sync) CSV `Video*` rows in `Pioneer DJ XDJ-RX2.midi.csv`, `XDJ-XZ (1).midi.csv`
- [ ] Mixer input selector feedback (USB A/B, phono, line, return, digital, Bluetooth) and peer USB port names CSV `InputSelect.* UsbA.* UsbB.* PeerUsbPortName.USBA/USBB`
- [ ] Channel and master level meter output to hardware CSV `ChannelLevel` (Indicator), `strings.txt: ChannelLevel.%d`
- [ ] Fader start and cross fader start (play/cue) CSV `ChannelFaderStartPlay ChannelFaderStartCue ChannelFaderStartHotCue CrossFaderStartPlay.A/B CrossFaderStartCue.A/B`
- [ ] Oscillator (sound generator) on DJM-S11/V10 class mixers CSV `OSC1On-OSC4On OSC.Parameter OSC.Volume OSCAssign.*` (this "OSC" is an oscillator, not Open Sound Control)
- [ ] Fail-safe state from hardware (controller signals that it took over audio) CSV `FailSafeState`

### 2.3 Jog wheels, platters and displays

- [ ] Jog signal processing: scratch, pitch bend, search, scan, smoothing, integrators, RPM and spin speed, ring buffers of jog counts and timestamps, five timestamp processor generations `DjController::*` (141 members: `ScratchProcessor ScratchProcessor3 SearchProcessor ScanProcessor SmoothProcessor Integral2-4 Rpm SpinSpeed BendCount JogCountSmoother JogCountRingBuffer AudioCallbackTimeStampProcessor..5`)
- [ ] Jog modes: vinyl (touch scratches) vs CDJ (touch ignored), jog cutter, jog sampler mode, background play `ControllerJog`, `ControllerJogCutter`, `djplay::JogCutterManager`, `strings.txt: JogCutterHalveSpeed JogCutterJumpLastCue JogCutterThresholdBpmToHalve _kJogSamplerMode JogBackgroundPlay`
- [ ] Vinyl speed adjust (touch/brake, release/start), back spin length, jog start time, anti drift `ControllerVinylSpeedAdjust`, `rbxfrm::VinylSpeedAdjustor`, `ControllerBackSpinLength`, `ControllerJogStartTime`, `strings.txt: JogAntiDriftSetting`, CSV `VinylAdjustBrake.Left/Right VinylAdjustTouchRelease`
- [ ] Absolute platter position and timestamped jog (DDJ-REV7) CSV `JogRotateAbsPos` (`B019`), `JogTimestamp` (`E0`, pitch bend status) in `DDJ-REV7.midi.csv`
- [ ] Jog ring/illumination LEDs (play position ring, cue countdown, current cue offset) CSV `JogPlayPauseIndicator` (JogIndicatorHiRes, Min=1 Max=135), `JogPlayPauseIndicatorShort` (Max=72), `JogCueIndicator`, `JogCurrentCue` (IndicatorHiRes), `OnJogIndicator`, `ControllerJogIllumination`
- [ ] On-jog displays (artwork, custom image, time mode, brightness, ring colour, beat scaler) `ControllerJogDisplay1-3`, `ControllerJogDisplayArtWorkComponent`, `ControllerJogDisplayRingColorComponent`, `DJSystemImageOnJogDisplay: setArtwork setCustomImage`, `strings.txt: JogDispArtWorkIsOn JogDispBeatScalerIsOn JogDispTimeMode JogRingColor JogRingCustomColor`
- [ ] Player/controller screen data over HID (waveforms 3-band and blue, artwork, cue points, beat grid, browse lists, track info, countdowns, mix point link state, key) `djplay::DisplayDataManager` (154 members), `djplay::ControllerDisplayBuilder` (CDJ-2000, 400, 850, 900, 900NXS, XDJ-1000, XDJ-RX field maps), `djplay::ControllerDisplay2Builder` (CDJ-1500X, CDJ-2000NXS2 incl. v1.30 variant, CDJ-3000, CDJ-3000X, DDJ-1000, DDJ-FLX10, DDJ-REV7, DJM-S11, OMNIS-DUO, OPUS-QUAD, XDJ-1000MK2, XDJ-AN, XDJ-AZ, XDJ-RX2, XDJ-RX3, XDJ-XZ field maps)
- [ ] Secondary HID "Sub" interfaces for screens on DDJ-1000, FLX10, REV7, DJM-S11, OMNIS-DUO, OPUS-QUAD, XDJ-AN/AZ/RX/RX2/RX3/XZ `djplay::HidMap*Sub: sendWholeWaveformData on3BandDetailedWaveformChanged onLargeAlbumArtChanged`
- [ ] Text displays for older CDJs `djplay::CDJ350TextBuilder`, `CDJ850TextBuilder`
- [ ] Player MySettings and DJM MySettings files and remote push `cdjDeviceSetting::DeviceSettingFile`, `pioneerMySetting::DJMMySettingFile`, `djplay::RemoteSettingController: sendDJMMySetting sendMySettingsToPlayer`, `strings.txt: /PIONEER/MYSETTING.DAT /PIONEER/MYSETTING2.DAT /PIONEER/DEVSETTING.DAT`

### 2.4 Pads (hardware side)

- [ ] Up to 16 pads per deck, 8 pad modes per layer with page left/right and parameter left/right buttons CSV `PAD1..16_PadMode1..8`, `PageLeft_PadMode*`, `Parameter1Left_*`
- [ ] Pad mode permission flags sent to the device (which page changes are allowed) CSV `Permission_PadMode1-8` (Indicator)
- [ ] Pad mode catalogue (from `Res/pad/Mode.pad.csv`): HOT CUE, PAD FX1, BEAT JUMP, SAMPLER, KEYBOARD, KEY SHIFT, BEAT LOOP, PAD FX2, SLICER, ACTIVE CENSOR, SEQUENCE CALL, PAD FX Combination, LIGHTING, TRANSPORT, SP.SCRATCH, STEMS, USER 1-8, Combination 1-9. Line format `key=value` pairs: `function,type(preset|user|combination|separator),mode,template,pad,text,select`
- [ ] Per-mode pad contents (`Res/pad/*.pad.csv`, lines `function=X,value=Y` with optional `userfunction=,uservalue=`), including the Lighting pad page: COOL, NATURAL, HOT, SUBTLE, WARM, VIVID, CLUB 1, CLUB 2, RED, GREEN, BLUE, WHITE, BLACK OUT, STROBE (FAST), STROBE (SLOW), STROBE (OFF) `Res/pad/Lighting.pad.csv`
- [ ] Velocity-sensitive sampler pads (DDJ-RZX and others) CSV `PAD*_VelocitySampler+Velocity`, `+VelocityRzx`
- [ ] Pad colour/LED feedback per mode **(inferred)** `djplay::PadDeviceUnit`, `KeyboardView::updateKeyBoardPadsColour`

### 2.5 MIDI Learn

- [ ] MIDI Learn window with device selector, tabs DECK, PAD, FX, SAMPLER, MIXER, BROWSE, OTHER, VIDEO, LIGHTING, MIXPOINTLINK `MidiLearnBinaryData: temp_binary_data_deck ... _video _lighting _mixpointlink`, embedded `<MIDI_COMMAND_DATA>` sections in `strings.txt`
- [ ] Table columns: Function, CSVNAME, Deck, MIDI IN, Type, Encoder Mode, MIDI OUT, Color, Comment (`<MIDI_LEARN_TABLE_DATA><COLUMNS>` in `strings.txt`), with editable cells per column `MidiLearnTableComponent::{DeckColumnCustomComponent, TypeColumnCustomComponent, EncoderModeColumnCustomComponent, MidiEditableTextCustomComponent, CommentEditableTextCustomComponent, MidiLearnCustomComboBox}`
- [ ] Type list shown to the user: Button, Button(for Pad), Rotary, Knob/Slider(0h-7Fh), Knob/Slider(0h-3FFFh), Indicator `strings.txt` (block after `@MidiLearn`)
- [ ] Learn button (capture next incoming message), Add, Delete, Duplicate, Default (restore built-in), New file, Import, Export, Save; duplicate-assignment warning `MidiLearnComponent: ttStrLearnButton ttStrAddButton ttStrDuplicateButton ttStrDefaultButton importMidiCsv exportMidiCsv deleteMidiCsv showAlertIfDuplicated msgStrAlreadyAssigned`
- [ ] Learnable command vocabulary: 711 commands with display name, CSV name, type code, category and per-deck flag (embedded `<COMMAND displayname csvname type category deck/>`). Categories include HotCue, PadFX, Keyboard, Loop, FX1/FX2, Sampler, VelocitySampler, STEMS, Slicer, KeyShift, Sequence, BeatJump, BeatLoop, SampleScratch, ActiveCensor, Tempo, CFX, TransitionFX, TouchFX, Text/Image/Camera, DVS, MFX1/MFX2, Grid, MemoryCue, AbletonLink, Capture, and lighting categories Bank, Mood, Color, Strobe, DeckSelect, AmbientMode, DMXDirectControl, DMXDirectControlKnob
- [ ] User mapping files created from defaults, stored per device, "unsupported format" error on bad import `DeviceDefs: createUserDefaultCsvFile getUserMidiCsvFile getDefaultMidiCsvFile`, `MidiLearnComponent::msgStrNotSupportedFormat`
- [ ] MIDI Learn for non-Pioneer MIDI devices (generic MIDI) `strings.txt: You can map each function with MIDI LEARN even on MIDI equipment other than HardwareUnlock devices.`
- [ ] Old-style per-device MIDI channel and MIDI key mode settings for DJMs via the PDJ driver API `-[PDJFunc midiChannelChanged:] midiKeyModeChanged:`, `strings.txt: PDJ_SetMidiChannelIndex PDJ_SetMidiKeyModeIndex`, `MySettingMidiCHComponent`, `MySettingMidiButtonTypeComponent`

### 2.6 Mixers (DJM hardware side)

- [ ] DJM as audio interface with per-model channel routing behaviours (DJM-S11, S7, V10, DDJ-REV7) `djplay::AudioPrefBehaviorDJMS11/S7/V10/DDJREV7`, `AudioOutputChannelBehaviorDJMS11/S7/V10`
- [ ] Mixer mode (internal mixer vs external mixer) and fixed mixer modes per device `DeviceDefs: getDefaultMixerMode getFixedMixerMode`, `ControllerMixerMode`, CSV `MixerModeSetting`, `-[PDJFunc mixerModeChanged:]`
- [ ] Setting Utility integration via per-model `*Setup.framework` (DJM-250MK2, 450, 750MK2, 900NXS2, S7, S9, S11, TOUR1, V5, V10, generic PioneerDJMSetup) for USB input routing and boost level `strings.txt: DJM-S11Setup.framework ... PioneerDJMSetup.framework`, `nonframework_pdjfunc: PDJ_SetUSBInputAudio PDJ_GetMixerInputSelectorStatus`, `-[PDJFunc boostLevelChanged:] inputAudioChanged:`
- [ ] Mixer input selector status read back and shown (USB A/B per channel) `PDJ_GetMixerInputSelectorStatus`, CSV `UsbASelectState.*`
- [ ] DJM MySettings over Pro DJ Link `PSvLinkDJMMySetting::ReadReq WriteReq DuplicateReq`, `pioneerMySetting::DJMMSData DJMMSDataBFX`
- [ ] Send/return FX channels on supporting mixers `DeviceDefs: getNumSendReturnChannels has4chSendReturnFx isEnableSendReturnFx`, `strings.txt: @SendReturnOnOff%d @ChangeSendReturnRoutingCh%d`
- [ ] Mic routing and talkover feedback (mic on/off, low latency mode, EQ, level) `MicOnOffDeviceCallback MicLowLatencyModeDeviceCallback MicEqDeviceCallback MicLevelDeviceCallback`
- [ ] On-air indication to/from hardware `OnAirWatcher`, `rbxfrm::OnAirStateServer`, CSV `OnAirIndicator`
- [ ] DJM-specific colour FX filter/noise behaviour matching the DJM-900NXS2 `ColorFxFilterDjm900NXS2`, `ColorFxNoiseDjm900Nxs2` (DSP owned elsewhere; listed so the hardware-matching flag is not lost)
- [ ] DVS control signal setup per hardware (absolute/relative/internal/through/wireless modes) CSV `DvsState DvsAbsRelInt VinylState`, MIDI commands `DvsAbsolute DvsRelative DvsInternal DvsThrough DvsWireless(A) DvsWireless(B)`, `DVSControlHardwareComponent` (decoder owned by the engine spec)

### 2.7 Other hardware integrations

- [ ] KUVO now-playing gateway over HID (sends track metadata, on-air status, ISRC, account key) `djplay::HidMapKUVO`, `HidKuvoManager`, `djplay::Kuvo*Builder`
- [ ] Phase wireless DVS (Essential, Pro, Pro Receiver) with battery, deck assignment, end warning `djplay::HidMapPhaseEssential(Deck)`, `HidMapPhasePro(Deck)`, `MidiMapPhasePro`, `djplay::phase::HidMapPhaseDeckAssignManager`, `PhaseDeviceConnectionListener`, `strings.txt: @PhaseBatteryLevel @PhaseLowBattery @PhaseEndWarning`
- [ ] INTERFACE 2 (DVS audio interface) authentication and control `djplay::HidMapINTERFACE2`
- [ ] Ableton Link hardware touchpoints: Link on/off and BPM up/down/tap from MIDI and keyboard `ControllerAbletonLink`, MIDI commands `AbletonLinkTapBpm AbletonLinkBpmUp AbletonLinkBpmDown AbletonLinkBpmUpDown`, `ableton::AbletonLinkButton::AbletonLinkKeyboardShortcut`
- [ ] OSC status monitor ("DJ monitor"): JUCE OSC receiver/sender; registered OSC listeners receive player status, mixer status and now-playing info; listener registration expires; gated by account/subscription; logs exported to analytics `djplay::djmonitor::{DJMonitor::oscMessageReceived, DJStatusSender::sendOSCMessage updatePlayerStatus updateMixerStatus, DJListenersController::addOscListener expirationOscListeners judgementOscRequest codecCertificationKey, _playerstatus _mixerstatus _playsonginfo _listenersstatus}`, `juce::OSCMessage`
- [ ] Philips Hue, Art-Net, sACN, StageLinQ, TCNet: **no evidence** in 7.2.19 (zero matches in `strings.txt` for `Philips`, `Art-Net`, `ArtNet`, `sACN`, `StageLinQ`, `TCNet`). Not required for parity.

### 2.8 Keyboard shortcuts

- [ ] Command-based shortcut system with an editor (JUCE KeyPressMappingSet style) `rb::KeyMappingEditorComponent`, `strings.txt: KEYMAPPINGS`
- [ ] Command categories: browse, deck1-4, deckCommon, editActionPad, editPlayer, editToolArea, file, fx1, fx2, fxCommon, general, help, kuvo, lighting, linkExport, mixer, playerA, playerB, playlist, rec, sampler, track, video, view `rb::KeyMappingEditorComponent::Categories`
- [ ] Named mapping files with presets (Export mode, Performance mode, a Serato-style preset, none) and user copies: copy, rename, delete, import, export, reset to defaults `KeyMappingFileManager: isPresetExport isPresetPerformance isPresetSerato isPresetNone copyKeyMapping renameKeyMapping importKeyMapping exportKeyMapping`, `KeyMappingFile`, `KeyboardPreference: copy erase rename reset read write`, `strings.txt: KeyMappings/, _kPerformaceKeyMappingDefaultValue`
- [ ] Modifier requirement for some new shortcuts, plan-gated shortcuts `strings.txt: Please use control, command or option in a new shortcut key combination.`, `Invalid operation. Keyboard shortcuts of this function are unavailable with your current plan.`
- [ ] Additional command targets: lighting, Groove Circuit, Ableton Link, video panel `dmx::LightingKeyboardShortcut`, `djplay::GrooveCircuitKeyboardShortcut`, `VideoPanel: getAllCommands getCommandInfo perform`
- Note: `djplay::KeyboardView` is the on-screen KEYBOARD pad mode (pitch play with hot cue pads, scale, octave), not keyboard shortcuts. It belongs to the performance spec; its controller inputs (`@PianoPlay%+d @PianoPlayScale @PianoPlayOctave`) are covered by the mapping layer here.

### 2.9 Lighting mode

Application and preferences:

- [ ] Lighting as an application mode alongside Export/Performance/Edit; switching stops playback with confirmation `strings.txt: Change EXPORT/PERFORMANCE/LIGHTING/EDIT mode, Are you sure you want to stop playing and switch to Lighting mode?`
- [ ] Lighting not available while Pro DJ Link is active, except "PRO DJ LINK Lighting" `strings.txt: Lighting mode is not available while Pro DJ Link is active., Enable PRO DJ LINK Lighting`
- [ ] Preferences: enable lighting, delay compensation (ms), DMX output interface choice (ENTTEC or RB-DMX1), display lighting thumbnail on decks, decks 3/4, fader following, suspend, quantise, auto start ambient, CDJ lighting and master-player priority `ExtensionEnableLighting ExtensionLightingDelay ExtensionOutputDMX ExtensionDisplayLighting ExtensionPlayMacroVenue ExtensionResetScene`, `strings.txt: LightingEnable LightingDelayTime LightingDmxIfEnttec LightingDmxIfRbDmx1 LightingEnableThumbnail LightingEnableDeck34 LightingEnableFader LightingEnableSuspend LightingQuantize LightingEnableAutoStartAsync LightingEnableCDJLighting CDJLightingPrioritizeMasterPlayer`

Fixture library and venues:

- [ ] Fixture library downloaded from AlphaTheta servers, with update check and retry `djplay::FixturesDataManager: downloadFixtureLibrary queryFixtureLibraryUpdate fixtureLibraryUrl fixtureLibraryVersionInfo parseFixturesFile`, `strings.txt: Fixture Library is updated. Do you want to download it?, https://dl.rekordbox.com/application/test/rbLighting/` **(URL association inferred)**
- [ ] Browse by brand, search and category filter `djplay::BrandAndFixturePanel`, `djplay::FixtureSearchFilter: getLightSearchCategory`, `djplay::FixtureDetailView`
- [ ] Venues as tabs: add, delete (with cascade warning), rename, copy, enable, last selected `djplay::VenueTabbedComponent::{AddVenueTabButton, DeleteVenueTabButton, VenueCopyPanel, VenueRenamePanel}`, `strings.txt: If you delete a Venue, all the related data will be deleted.`
- [ ] Fixture map: drag fixtures onto a 2D room map, move, delete, undo/redo `djplay::FixtureMapBox`, `FixtureMapComponent`, `DragableFixture::FixtureMoveUndoAction`, `FixtureDataListModel::{FixtureAddUndoAction, FixtureDeleteUndoAction}`, `FixtureMapUndoManager`
- [ ] Per fixture: name, library fixture, DMX mode, universe, start address, colour number, order, macro role (which of the 25 macro fixture slots), pan/tilt offset, pan/tilt limits, tilt reversal (user DB `fixture` table, section 3.4)
- [ ] Moving head initial position setup with live preview against real fixtures `djplay::MovingHeadSettingPanel::{AlgmList, TrackPreview}`, `strings.txt: Moving Head initial position setting, Check the selected pattern with the actual moving heads`
- [ ] Strobe settings, gobo settings dialogs `djplay::StrobePanel`, `djplay::StrobeSelector`, `strings.txt: Display the Gobo settings dialog., Display the dialogue for Strobe setting.`
- [ ] DMX Direct Control: 9 user buttons per venue, each driving one DMX channel with on/off values, auto-on, on-with-blackout, optional knob control, notes `djplay::DMXDirectSettingPanel`, `djplay::DMXDirectController`, `direct_control` table, `strings.txt: When on, the DMX channel settings can be controlled with the MIDI knob/fader.`

Macro mapping and editing:

- [ ] Automatic scenes from phrase analysis: each track gets a macro pattern (mood x bank); each phrase gets a macro (scene) `lighting_db::LightingDatabaseIF: getMacroPatternId getMacroAssign getPhraseData`, `dmx::SongStructureBasedLightingData`, `dmx::LightingDataLoader: loadLightingDataByContentId loadLightingDataByMacroPatternId`
- [ ] Moods High/Mid/Low and banks Cool, Natural, Hot, Subtle, Warm, Vivid, Club 1, Club 2; ambient/interlude scenes 1-6 `macro.db3` (section 3.4), `SceneCategoryPanel: moodNameList bankNameList`
- [ ] Macro Mapping tab: per-track phrase to scene assignment with drag and drop, reset relations, animated scene thumbnails, undo/redo `djplay::LightingPanel::MacroMapping::{SceneEditor, ShowData, AnimationComponent}`, `MacroMappingUndoManager`, `strings.txt: Reset all the relations between Phrases and Scenes.`
- [ ] Scene categories: add scene (macro), delete, reset preset scene, copy `djplay::SceneCategoryPanel::{AddMacroUndoAction, DelMacroUndoAction, ResetPresetMacroUndoAction, CategoryItem::SceneCopyPanel}`
- [ ] Macro Editor tab: per-fixture timeline over the track waveform with brightness curve, colour blocks, strobe blocks, moving-head movement blocks, rotate blocks and gobo blocks, add/delete/move edges, undo `djplay::LightingPanel::MacroEditor`, `CLCEStrobe CLCEStrobeBlock CLCEMovingHead CLCEMovingHeadBlock CLCEMovingHeadParams CLCEGobo CLCEGoboBlock`, `djplay::LightingSceneEditPanel`, `djplay::SceneEditToolBar`, `djplay::LightingPhraseModel::LightingPhraseCell::{CellColorData, CellGoboData, CellMovingHeadData, CellPointData, CellRotateData, CellStrobeData}`
- [ ] Per-track custom lighting (overrides) stored as `lighting_data` per content and fixture `UserRepository: insertLightingData`, `onContentLightingDataModified`
- [ ] Lighting thumbnails on the deck waveform (per phrase) `djplay::LightingThumbnailView`, `djplay::LightingThumbnailSelection`
- [ ] Import/export of the lighting database `LightingPanel: executeLightingDbExport executeLightingDbImport`, `LightingDatabaseIF::importDBFile`, `strings.txt: Export lighting settings, Import lighting settings`

Live performance:

- [ ] Lighting control panel: mood pads, bank pads, colour pads (Red, Green, Blue, Cyan, Magenta, Yellow, White, user colour), strobe (fast, middle, slow, off), blackout, auto mood/bank/colour/strobe, deck select (auto, decks 1-6), master dimmer (grand volume), ambient (interlude 1-6, off, BPM-synced ambient), DMX direct 1-9 and knobs 1-9 `djplay::LightingControlPanel`, `dmx::DMXBuilder: onMoodPadPressed onBankPadPressed onColorPadPressed onStrobePadPressed onAllOffPadPressed onAsyncPadPressed onDMXDirectPadPressed onDimmerValueChanged forceDeckSelect`, MIDI commands `Lighting*` (section 3.1.6)
- [ ] Shutter open/close, forced macro/mood/bank/colour/strobe overrides with restore `dmx::DMXBuildConfiguration: forceMacro restoreMacro setShutterStatus setMacroMode setSongMode setDirectMode`
- [ ] Lighting clock synchronised to deck beat position (play/stop, PPQ position, BPM, loop beat) `dmx::LightingClockGenerator: reqPositionPpq setCurrentBPM setLoopBeat`, `dmx::LightingClockController`
- [ ] Automatic target deck selection (by on-air, fader, master) **(inferred)** `DMXBuilder: getActiveDeckNo isPossibleAutoSelection getMixerChannelOrder`, `strings.txt: Automatically selects the lighting performance player.`
- [ ] Pad LED feedback for lighting pads on controllers `DMXBuilder: updateMoodPadLed updateBankPadLed updateDMXDirectPadLed updateDeckSelectPadLed updateAllOffPadLed`, `dmx::DMXPadStateListener`
- [ ] CDJ lighting: phrase data and play state received from CDJs/XDJs over Pro DJ Link drives scenes `djplay::CDJLightingLinkManager`, `djplay::CDJLightingPlayer: receiveSongStruct getPlayerPhrases PhraseConverter`, `dmx::LightingDataLoader: musicChangedFromCDJ`
- [ ] Plan/subscription gating of lighting `DMXBuilder: onPlanDowngrade onSubscriptionStatusChanged onServerLicenseChanged` (behaviour to mirror as a no-op; see section 6)

DMX output:

- [ ] Interfaces: RB-DMX1 (HID, authenticated), ENTTEC DMX USB Pro, Enttec Open DMX USB, DMXKing ultraDMX, FTDI USB COM485 Plus1, Vince USB-DMX512 (FTDI D2XX), generic HID DMX devices, a DDJ with internal DMX `djplay::HidMapRBDMX1`, `EnttecDMXUSBPro`, `EnttecDMXUSBOpen`, `DMXUSB`, `DMXUSBWidget`, `FTD2XXInterface`, `HIDDMXDevice`, `HIDPlugin`, `DDJDMXDevice`, `DDJPlugin`, `strings.txt: DMXKing ultraDMX range, Enttec DMX USB Pro, Enttec Open DMX USB, FTDI USB COM485 Plus1, Vince USB-DMX512, DDJ-InternalDMX`
- [ ] FTDI D2XX loaded dynamically from the bundled `libftd2xx` `Contents/MacOS/libftd2xx.1.4.30.dylib`, `FTD2XXLoader`, `strings.txt: FT_Open FT_SetBaudRate FT_SetBreakOn FT_SetDataCharacteristics FT_Write ...`
- [ ] Universe writing on a dedicated thread at a fixed frame rate ("DMX Frame Frequency") `dmx::DMXBuilder::writeToUniverse run`, `LightingIOPlugin::writeUniverse`, `strings.txt: DMX Frame Frequency`
- [ ] Plugin abstraction with inputs and outputs, hot plug, rescan `LightingIOPlugin: openOutput writeUniverse inputs outputs`, `DMXUSB: rescanWidgets onDeviceAdded onDeviceRemoved`, `LightingDeviceManager`

### 2.10 Video mode

- [ ] Enable video function; low-speed machine setting disables video `ExtensionEnableVideo`, `strings.txt: When [Low Speed] is selected, rekordbox video is disabled.`
- [ ] Video panel with two video decks (load deck 1/2), video crossfader tied to the audio crossfader, brightness following channel faders, per-deck mute, AV sync, delay time, quality `VideoPanel` (184 members: `loadToVideoDeck updateAudioCrossFaderProportion updateAudioChannelFaderProportion`), `VideoPanel::VideoCrossFaderProcessor`, `strings.txt: VideoBrightnessChannelFaderDeck1/2 VideoDeck1Mute VideoDeck2Mute VideoAvSync VideoDelayTime VideoQuality`
- [ ] Supported files: video `*.mp4 *.avi *.mpg *.mpeg *.mov *.m4v`, images `*.jpg *.jpeg *.png *.gif`, Quartz Composer `qtz` `strings.txt` file filters, `rb::MFVideoReader: openQTZ drawQTZ`, `imports.txt: _OBJC_CLASS_$_QCRenderer _OBJC_CLASS_$_AVAssetReader`
- [ ] Linked video per track, "when loading a track without video": random linked video, specified linked video, artwork, nothing; visualiser (SPHERE, TUNNEL, PARALLEL, PLANET, OCTAGON, POLYHEDRON, SQUARE) from a downloadable `visualizer.zip`; sample video download `ExtensionLinkVideo`, `ExtensionWithoutVideoComponent`, `ExtensionDownloadSampleVideo`, `strings.txt: Randomly display Linked videos, visualizer.zip, https://dl.rekordbox.com/application/visualizer/`, track column `VideoAssociate`
- [ ] Transition FX (ids 10-38): ADDITIVE, FADE, DIFFERENCE, LUMA KEY DARK, LUMA KEY BRIGHT, RGB, PUSH, WIPE H, WIPE V, SQUEEZE H, SQUEEZE V, GRID, CUBE, SPIN H, SPIN V, SWITCH, EXTREME CUT, EXTREME HOLD, SWAP, FLIP, STROBE, IRIS, GRID2, PIXEL, DOORS, ROLLING BLINDS, DROP, REC EXPANSION, SIN WAVE (shader mode comments in `strings.txt`)
- [ ] Video FX (ids 50-73): ZOOM, KALEIDOSCOPE, STROBE, EDGES, BLUR, TILES, SPLIT SCROLL, LIGHTBEAM, EXPOSE, FEEDBACK, STRIPE, FLOOD, BEN DAY, INCREMENT, DOT, RGB TRIP, RGB CYCLE, BEAT GLOW, NEON, DISPLACE, HEAT, LOREZ, TWIST, TWIST 2; colour adjust (ids 150-155): BRIGHTNESS, CONTRAST, COLOR ISOLATOR, FILTER, SATURATION, SHARPEN (same source)
- [ ] Transition selection, random, five favourites, auto transition left/right with speed, max transition count, active list `VideoPanel: startAutoTransition syncFavorite`, `strings.txt: VideoTFXActive VideoTFXAutoSpeed VideoTFXFavorite VideoTFXMaxNum`, MIDI `TransitionFxSelect TransitionFxRandom AutoTransitionLeft/Right TransitionFxFavorite1-5`
- [ ] Touch FX: XY pad over the preview triggering assigned Beat FX (24 slots) and Colour FX (6 slots) per deck or master, with hold `VideoPanel::TouchFxControl`, `VideoPanel::OverlayComp::{BeatFxController, ColorFxController}`, `ExtensionTouchFX`, `TouchFxSpecList`, `strings.txt: VideoTouchFxBeatFxAsign_1..24 VideoTouchFxColorFxAsign_1..6`, MIDI `TouchFxOn1/2/Master TouchFxHold1/2/Master`
- [ ] Overlays: text (font, animation, scroll), image (with animation), camera; opacity, size, position, rotation; preview; history; on/off; select next `videoPanel::{TextControlComp, ImageControlComp, CameraControlComp, SubComponent}`, `VideoPanel::Setting: getTextFont getAnimationValue getTransparencyValue getSizeValue addHistory`, MIDI `TextImageCamera*`
- [ ] Camera input via AVFoundation (640x480, 960x540, 1280x720 presets), device choice, USB camera monitoring `rb::CameraDevice::Pimpl`, `imports.txt: _AVCaptureSessionPreset640x480 _AVCaptureSessionPreset960x540 _AVCaptureSessionPreset1280x720`, `strings.txt: VideoUsbCameraMonitoring`
- [ ] Watermark image (plan-dependent) `VideoPanel: setWatermarkImage renderWatermarkImage`
- [ ] Output window: separate window or second screen, full screen, cursor auto-hide `videoPanel::MasterOutComp`, `VideoPanel::VideoWindow: enterFullScreen`, `strings.txt: VideoPopupWindowSecondScreen`
- [ ] Syphon server output `ExtensionSyphonOut`, `Frameworks/Syphon.framework`, `imports.txt: _OBJC_CLASS_$_SyphonServer`, `strings.txt: VideoSyphonOut, Syphon Out Setting`
- [ ] Karaoke: CDG decoding (packets, colour tables, tile blocks, scroll) from plain files and zip archives `rb::CDGFile: processPacket loadColorTable tileBlock scroll renderAtPosition`, `rb::CdgReader`, `rb::CdgFileIoStream`, `rb::CdgZipFileIoStream`, `strings.txt: Karaoke File`
- [ ] Video thumbnails and QTZ thumbnails for the browser `rb::ThumbnailGenerator: CreateBitmap CreateQtzImage`, `strings.txt: VideoBrowseEnable`
- [ ] Video controls from hardware (XDJ-RX2, XDJ-XZ) and transition index to HID displays `djplay::VideoTransitionFxBuilder`, `DisplayDataManager::getVideoTransitionFx`, `strings.txt: @VideoControlOn @VideoControlOff`
- [ ] Lyrics panel coupled to video panel visibility; lyrics acquisition with a 3,000-track limit; downloadable lyric font `VideoPanel::onLyricPanelVisibilityChanged`, `common::kLyricAcquired kLyricExceedsLimit kLyricAcquireFailed`, `strings.txt: https://dl.rekordbox.com/application/Lyric/rbLyricFont_Ver.txt`, `djmdContent.LyricStatus`. The lyrics provider is not identifiable from static evidence (open question).
- [ ] Rendering through OpenGL and Metal paths with a separate-shader option `VideoPanel: renderOpenGL buildShaderProgram`, `VideoPanelSS`, `MetalContext`, `strings.txt: VideoSeparateShader`

---

## 3. Formats and protocols

### 3.1 Mapping CSV (`*.midi.csv`)

All facts below were measured across the 69 files in `Res/MidiMappings/` (17,213 lines in total).

#### 3.1.1 File naming and encoding

- File name is `<device port name>.midi.csv`, where the name matches the CoreMIDI device/port name (for example `PIONEER DDJ-SX.midi.csv`, `Pioneer DJ XDJ-RX2.midi.csv`, `DDJ-ERGO MIDI.midi.csv`). Devices exposing two ports have two files: either a `" MIDI"` suffix (`XDJ-AZ MIDI`, `OPUS-QUAD MIDI`) or `" (1)"`/`" (2)"` (`XDJ-XZ (1)`, `XDJ-XZ (2)`). `2IN2OUT` variants are separate files with identical content.
- Encoding: plain ASCII or UTF-8 with a byte-order mark (`OMNIS-DUO`, `OPUS-QUAD*`, `XDJ-AN`, `XDJ-AZ*`). Line endings: CRLF in most files, LF in newer ones. A parser must accept both and strip a BOM.
- One file contains a full-width `＃` (U+FF03) comment marker (`＃JOGDIAL`); treat it as a comment.

#### 3.1.2 Line kinds

1. Header: `@file,<version>,<name>` with optional trailing empty fields. Version is `1` in every shipped file. The binary formats this line as `@file,%d,` (`strings.txt`), and parses with `MidiMap::analyzeCsvLineV1` and `analyzeCsvOptionV1` `classes.tsv`. `<name>` usually equals the file name stem, except `euphonia.midi.csv` which declares `EUPHONIA`.
2. Column caption (always line 2): `#name,function,type,input,deck1,deck2,deck3,deck4,output,deck1,deck2,deck3,deck4,option,comment`. It is a comment, but it fixes the 15-column layout.
3. Comment: first field starts with `#`. Used for section headings (`# Browser`, `# Deck`, `# Mixer`), for disabled mappings (`#JogSearch,JogSearch,Difference,B01F,...`) and for documenting physical controls deliberately left unassigned (`#,MasterLevel,KnobSliderHiRes,B608,...,No function assigned here`).
4. Separator: all fields empty (`,,,,,,,,,,,,,,`).
5. Mapping row: everything else. Rows have 15 fields; one row in `DDJ-FLX6.midi.csv` has 19 (extra trailing fields to be ignored).

#### 3.1.3 Columns

| # | Caption | Meaning |
|---|---------|---------|
| 0 | `name` | rekordbox function identifier (the "CSVNAME" in MIDI Learn). May carry a variant after a dot (`InputSelect.USBA`, `CrossFaderCurve.SHARP`, `FX1-1Select.ECHO`, `TempoRange.WIDE`) or a modifier suffix (`PAD1_HotCue+Shift`). The same name may appear on several rows (one per physical source, for example `Shift` on four channels). |
| 1 | `function` | Physical control label, optionally followed by `+`-joined modifiers. Observed modifiers: `Shift` (3,479 rows), `Press` (188), `LongPress` (64), `FX1-1..FX2-3` (held FX buttons, 5 each), `Stem` (4). Some rows leave it empty. 1,002 distinct base labels. **(inferred)** used by the MIDI Learn UI and to bind modifier state. |
| 2 | `type` | Control element type, section 3.1.4. |
| 3 | `input` | Incoming message as 4 hex digits: status byte then data byte 1 (`900B` = Note On, channel 1, note 0x0B; `B640` = Control Change, channel 7, controller 0x40). Empty when the deck columns carry full messages. Two special forms: `FFF1`..`FFF3` pseudo addresses on `Parameter` rows, and a 2-digit `E0` (pitch bend status, no data byte) on `JogTimestamp`. |
| 4-7 | `deck1..deck4` (input) | Either a decimal channel offset 0-15 added to the low nibble of the `input` status (`900B` with `0,1` = channel 1 for deck 1, channel 2 for deck 2), or, when `input` is empty, a complete 4-hex message for that deck (`Load,...,,9646,9647`). All empty means a global (non-deck) control. |
| 8 | `output` | Feedback message, same encoding as `input`. |
| 9-12 | `deck1..deck4` (output) | Same rules as 4-7, for feedback. |
| 13 | `option` | `;`-separated flags and `key=value` pairs, section 3.1.5. |
| 14 | `comment` | Free text shown in MIDI Learn (`Load to Deck / Instant Double (double click)`). |

Observed status nibbles in `input`/`output`: `9` (Note, 18,877 uses), `B` (CC, 2,154), `F` (pseudo, 140), `E` (1). Deck offset values observed: 0-14. Pads conventionally sit on channels 8-15: deck 1 normal `7` and shift `8`, deck 2 `9`/`10`, deck 3 `11`/`12`, deck 4 `13`/`14` (for example `PAD1_PadMode1,...,Pad,9000,7,9,11,13,...`).

#### 3.1.4 Types

Counts are rows across all files.

| Type | Rows | Message and behaviour |
|------|------|-----------------------|
| `Pad` | 5,868 | Note on/off from a performance pad, routed through the pad mode system. Velocity available for velocity sampler. |
| `Button` | 5,190 | Note on (pressed, 0x7F) and off or note-on 0x00 (released). Feedback is a note with LED value. |
| `KnobSliderHiRes` | 899 | 14-bit absolute CC: MSB on controller *n*, LSB on *n*+0x20 (standard MIDI pairing; the CSV names only *n*, for example `Gain B004`). **(inferred from MIDI convention and Pioneer MIDI lists)** |
| `Indicator` | 381 | Output only: LED or value feedback (for example `ChannelLevel`, `Permission_PadMode*`). |
| `Rotary` | 270 | Relative encoder CC (browse, FX select, beat up/down). Two's-complement 7-bit delta **(inferred)**. |
| `KnobSlider` | 232 | 7-bit absolute CC. |
| `Value` | 215 | Absolute value with explicit range (`NeedleSearch ... Max=16383`; mixer levels on DJMs). |
| `JogRotate` | 154 | Jog wheel tick delta, CC centred on 0x40 **(inferred from Pioneer MIDI lists)**. Separate CCs for scratch, pitch bend, search, wheel bend. |
| `Parameter` | 140 | Device configuration value sent at init; `input` is a pseudo address `FFFx`, value in `Value=`. |
| `JogTouch` | 78 | Note on/off from the jog touch sensor. |
| `Difference` | 39 | Relative delta for jog search. |
| `JogIndicator` | 25 | Jog ring LED position, range in `Min=`/`Max=`. |
| `Slider` | 6 | Absolute position (DDJ-RZX touch strips). |
| `IndicatorHiRes` | 5 | 14-bit feedback (jog cue countdown, current cue offset). |
| `RotaryAsKnob` | 4 | Endless encoder accumulated into an absolute knob value; `Value=2048` **(inferred: initial/centre value of a 12-bit or 14-bit range)**. |
| `JogIndicatorHiRes` | 3 | 14-bit jog ring position, `Min=1;Max=135`. |
| `RawValue` | 3 | Value passed through unscaled (`VideoTransitionFxIndex`). |
| `JogRotateAbsPos` | 1 | Absolute platter angle (DDJ-REV7, `B019`). |
| `JogTimestamp` | 1 | Pitch-bend-status message carrying jog timing (DDJ-REV7, `E0`). |
| (empty) | 4 | Present only on comment-like rows; ignore. |

The binary also knows `Toggle`, `SysEx` and variable MSB/LSB element classes that never appear as CSV types (`djplay::MidiToggle`, `MidiSysEx*`, `MidiVariableMsb/Lsb`), so they are created from code in per-device `MidiMap*` classes **(inferred)**.

#### 3.1.5 Options

| Option | Rows | Meaning |
|--------|------|---------|
| `Fast` | 7,565 | **(inferred)** Process on the MIDI receive thread instead of the message thread (`DeviceMidi: midiInData midiInDataForThread`), for latency-critical controls (transport, pads, faders). |
| `RO` | 1,485 | **(inferred)** Row is fixed: shown but not editable or deletable in MIDI Learn (applied to Shift, jog, fader start, device state rows). |
| `Dual` | 906 | Row participates in dual deck control. |
| `Priority=N` | 523 | Output priority for the MIDI output queue (`50`, `100`, `300` observed) `djplay::PriorityMidiMessage`. |
| `Value=N` | 152 | Parameter value or initial value. |
| `Blink=ms` | 74 | LED blink period (`250`, `500`, `600`, `1000`). |
| `Max=N` | 67 | Upper bound (`127`, `64`, `72`, `135`, `16383`). |
| `Min=N` | 28 | Lower bound (`1`). |

#### 3.1.6 Function vocabulary

There are 1,269 distinct `name` values in the shipped files; the MIDI Learn command list has 711 entries (section 2.5). Families (full lists are reproducible from the evidence with the script approach used for this spec):

- Transport and deck: `PlayPause Cue JumpToTrackStart TrackNext TrackPrev SearchFwd SearchRev Slip SlipReverse Reverse Vinyl Quantize AutoCue MasterTempo TempoSlider TempoRange(.6/.10/.16/.WIDE) TempoUp TempoDown TempoReset Sync SyncRate Master TapBpm PitchBendUp PitchBendDown KeySync KeyReset SemitoneUp SemitoneDown NeedleSearch Load LoadDdjRz ForwardAndLoad DeckMove DirectionFwdRev`
- Loops and jumps: `LoopIn LoopOut ReloopExit ActiveLoop AutoLoop 4BeatLoop LoopHalf LoopDouble LoopHalfDouble LoopMove LoopMoveLeft/Right BeatLoop1_64..BeatLoop512 BeatJump Jump4Fwd Jump8Prev ... ReTrigger ManualLoopMode AutoLoopMode`
- Cues: `Memory Delete CallNext CallPrev HotCue Parameter1Left_HotCue ...`
- Jog: `JogScratch JogPitchBend JogSearch JogTouch WheelPitchBend WheelSearch JogCutterMode JogJogCutterScratch InstantScratch BackSpinLengthSetting`
- Pads: `PAD<n>_<Mode>` and `PAD<n>_PadMode<m>` with `+Shift`, `PadMode1..8`, `PageLeft/Right_*`, `Parameter1/2Left/Right_*`, `Permission_PadMode*`, `PadInfoView.Left/Right`
- Mixer: `Gain EQHigh EQMid EQLow ChannelFader CrossFader CrossfaderAssign(.A/.B/.Thru) CrossFaderCurve* ChFaderCurve.* HeadphoneCue MasterCue HeadphonesMix HeadphonesVolume MasterLevel ChannelLevel(.1-.5) IsolatorMode EqCurve Mute PartIsolator*`
- FX: `FX1-1..FX2-3 (+On, +Select, +Select.<FXNAME>, +BeatUp/Down, +OnMomentary) FX1BeatUp/Down FX1Assign* FX1ReleaseFX* CFX* SmartCFX* SFX1/2-* MFX1/2* DrumRoll* DrumTrans* DrumRelease*`
- Sampler and sequencer: `Sampler* SamplerVolume SamplerGain SamplerCue SamplerSync SequencerRec SequencerStart SaveSequence SequenceCall SequenceLoad SelectSlot SlicerCapture Capture`
- Browse and view: `Browse BrowseUp/Down BrowseJump* Forward Back Area* AddToTagList RelatedTracks Discover TrackFilter Sort* SwitchActiveWindow Zoom WaveFormView* DeckView* TrackInfoView* PanelForward/Back FxPanel SamplerPanel`
- Mix Point Link: `MixPointLink MixPointLinkDeck MixPointLinkExit MixPointLinkSilent MixPointSelectNext/Prev`
- Device state and settings: `DeckState DualDeckState13/24 FailSafeState DvsState VinylState PlayingState LoadedIndicator* EndWarningIndicator BeatIndicator* BpmInformation ShiftFuncDisp *Setting`
- Video: `Video*` (section 2.10)
- Lighting (MIDI Learn only, not in shipped CSVs): `LightingAutoMood LightingHigh/Mid/Low LightingAutoBank LightingCool/Natural/Hot/Subtle/Warm/Vivid/Club1/Club2 LightingAutoColor LightingRed/Green/Blue/Cyan/Magenta/Yellow/White LightingUserColor1 LightingAutoStrobe LightingStrobeFast/Middle/Slow/Off LightingAllOff LightingMasterDimmer LightingDeckSelectAuto LightingDeckSelectDeck1-6 LightingAsync1-6 LightingAsyncOff LightingDMXDirectControl1-9 LightingDMXDirectControlKnob1-9`

MIDI Learn type codes in the embedded command XML (inferred by cross-referencing which commands carry which code): `1` button (296 commands), `2` rotary/relative (14), `3` knob/slider (45), `5` repeat-style button (`TempoUp`, `TempoDown`), `6` pad button (337), `7` value (`NeedleSearch`, velocity) (17).

#### 3.1.7 Device-pushed and internal messages

Strings starting with `@` name internal events that per-device `MidiMap*`/`HidMap*` classes send or receive, for example `@PlayStatusNotify @BeatNotifyDJM @BpmInfo @PadModeInfo @FX%d-%dOn @SlipOnPlay1 @CuePauseOn1 @JogCount @JogScratch @CurrentTime @TotalTime @MixPointLinkRemainingTime @HotCue @KeyShiftValue @PianoPlay%+d` (`strings.txt`). They are the vocabulary of the display/feedback layer, not CSV rows.

### 3.2 Pad definition files (`Res/pad/*.pad.csv`)

Line-oriented, comma-separated `key=value` pairs (no header).

- `Mode.pad.csv` keys: `function` (mode id), `type` (`preset`, `user`, `combination`, `separator`), `mode` (`on`/`off`: selectable as a hardware pad mode **(inferred)**), `template` (`on`/`off`: usable as a pad editor template **(inferred)**), `pad` (`on`/`off`: usable as a single pad assignment **(inferred)**), `text` (display label), `select` (hardware pad mode button number 1-8, or empty).
- Content files (`HotCue`, `BeatJump`, `Lighting`, `Stems`, `User1..8`, `Combination1..9` and so on): one line per pad with `function=<mode>,value=<arg>`, optionally `userfunction=<UserMode>,uservalue=<arg>` (`BeatJump`, `KeyShift`).

These files belong to the pad editor spec; they are listed here because the controller layer reads the `select` numbers.

### 3.3 HID

What the evidence shows:

- Transport: IOHIDManager matching on vendor/product, input report callbacks, `IOHIDDeviceSetReport` for output `imports.txt`.
- Per-model report codecs: `djplay::HidMap*` classes with `inputUnit`, `outputUnit`, `outputUnitSysEx`, `doOperate*Change` and `getHidOutputData` `classes.tsv`. Old vs new framing: `HidMap::isOldHidFormat`, `djplay::HidExtentFormat`, `HidReportRingBuffer`.
- Bulk display data (waveforms, artwork, browse lists) is chunked into "extents" (`ControllerDisplayBuilder::getNextExtent`, `DMXDataBuilder::getNextExtent`, `VideoTransitionFxBuilder::getNextExtent`) **(inferred: large payloads split across many HID reports)**.
- Authentication: devices may require a challenge/response handshake before full operation (`djplay::DeviceAuth`, `DeviceAuth2`, `HidMap::getDeviceAuth getHidAuthFormat inputUnitAuth`, strings `@AuthChallengeA @AuthResponseA @AuthResponseE SeedE hashA`). This is AlphaTheta's licence/authenticity mechanism. Cuepoint does not implement or bypass it; see section 6 and open question Q7.

Report layouts, vendor/product IDs and field maps are not recoverable from strings. Public partial references exist for CDJ HID (community projects) but must be treated as untrusted; primary source should be USB captures of our own hardware (section 8.4).

### 3.4 Lighting data

#### 3.4.1 Shipped macro database (`Res/LIGHTING_DB.zip`)

The zip holds one file, `macro.db3` (SQLite 3.27, 9.76 MB, dated 2020-09-21). The binary installs it as `LightingDB/macro.db3` next to a user database `LightingDB/user.db3` (`strings.txt`). Despite the resource name it contains preset scenes, not the fixture library.

Tables (row counts in brackets):

- `macro_fixture(id, name, fixture_type_id)` [25]: abstract fixture roles that macros are written for: `Par Light 1-4` (type 1), `Bar Light 1-6` (type 2), `Moving Head 1-4` (type 3), `Strobe` (4), `Mirrorball Spot` (5), `Effect 1-2` (8), `Laser` (9), and "Simple" variants `Par Light 1-2 (Simple)` (101), `Bar Light 1-2 (Simple)` (102), `Moving Head 1-2 (Simple)` (103). Types 6 and 7 are unused here.
- `macro(id, name, beats, fixed, thumbnail, preset, enabled)` [183]: one scene. Names are `<MOOD> <PHRASE> <BANK>` (for example `HIGH INTRO1 COOL`, `MID VERSE3 WARM`, `LOW BRIDGE VIVID`, `CHORUS CLUB1`, `INTERLUDE 4`). `beats` is 32 for all scenes (0 for a `SEPARATOR` row, id 10000). `thumbnail` names a frame image (`001_H_B1_FrameID.png`). Ids 61-66 are empty disabled placeholders.
- `macro_data(id, macro_id, macro_fixture_id, data)` [4,520]: the scene content per fixture role, as XML (below). Every enabled macro has data for all 19 standard roles (182 each) and 177 for the six Simple roles.
- `macro_pattern(id, energy, pattern)` [27]: energy `1`=High, `2`=Mid, `3`=Low; pattern `1`-`6` = Cool, Natural, Hot, Subtle, Warm, Vivid; `7`, `8` = Club 1, Club 2; `99` = Interlude (ambient).
- `macro_assign(macro_pattern_id, phase, macro_id, initial_macro_id)` [232]: which scene plays for each phrase slot of a pattern. Phrase slots by mood (from the names): High: 1 Intro1, 2 Intro2, 3 Up1, 4 Up2, 5 Up3, 6 Chorus1, 7 Chorus2, 8 Down, 9 Outro1, 10 Outro2 (11 present but empty). Mid: 1 Intro, 2-7 Verse1-6, 8 Chorus, 9 Bridge, 10 Outro. Low: 1 Intro, 2 Verse1, 3 Verse2, 4 Chorus, 5 Bridge, 6 Outro. Interlude: 1-6. These match rekordbox phrase analysis kinds (owned by the analysis spec). `initial_macro_id` equals `macro_id` in the shipped data and supports "reset to preset".
- `macro_event(macro_data_id, kind, beat_num, sequence_num, value1, interval, value2)` [0]: unused in shipped data.

The binary additionally reads `macro_property` and `macro_update` tables if present (`SELECT * FROM macro_property WHERE key = :key`, `select * from macro_update`, existence check via `sqlite_master`), so newer macro DBs can carry versioning and incremental updates.

#### 3.4.2 Scene XML (`macro_data.data`, `lighting_data.data`)

Root `<LightingEditModel ver="1.0">` with child tracks. All `x*` values are beat positions within the scene (0 to `beats`, fractional allowed). Observed vocabulary:

- `<Brightness><PointBlock xleft xright><Point x y type/>...</PointBlock></Brightness>`: a piecewise-linear brightness envelope, `y` in 0..1. `type` 1 = first point, 2 = interior, 3 = last point **(inferred from ordering)**.
- `<Colour><ColourBlock xleft colourleft xright colourright/>...</Colour>`: colour over time; colours are signed 32-bit ARGB integers (`-16776961` = 0xFF0000FF, blue; `-65536` = red; `-1` = white; `-16777216` = black). Left and right colours allow fades **(inferred)**.
- `<Strobe><StrobeBlock xleft strobeleft xright stroberight/></Strobe>`: strobe intensity/rate 0..1.
- `<Position><MovementBlock .../></Position>` (moving heads): `pattern` (`Circle`, `Line`, `Line2`, `Square`, and a misspelt `Squar` in data), `width`, `height` (0-127), `offset_x`, `offset_y`, `round_angle`, `offset_angle`, `start_angle`, `period_time` (ms), `frequency_x`, `frequency_y`, `phase_x`, `phase_y`, `type` (`Loop`, `SingleShot`, or `0`), `relative`, `direction` (`Forward`, `Backward`, or `0`).
- `<Rotate><RotateBlock xleft rotateleft xright rotateright/></Rotate>`: rotation speed/position 0..1 for effect fixtures.
- Gobo: present in code (`CLCEGobo`, `CLCEGoboBlock`, `dmx::Gobo`, `SongStructureBasedLightingData::parseGobo`, string `GoboBlock`) but not in shipped data. Element name `GoboBlock`, attributes unknown (open question Q12).

Whitespace and line endings vary (LF and CRLF); numbers appear with up to 20 significant digits. A parser must be tolerant; a writer should emit canonical short floats.

#### 3.4.3 User lighting database (`LightingDB/user.db3`)

Schema reconstructed from SQL strings in `strings.txt`:

- `venue(id, name, order, enabled)`
- `fixture(id, name, venue_id, fixture_master_id, mode_num, macro_fixture_id, universe_num, start_addr, color_num, order, offset_x, offset_y, limit_min_x, limit_max_x, limit_min_y, limit_max_y, tilt_reversal)`; the last seven columns are added by `ALTER TABLE` migrations.
- `direct_control(button_num, name, venue_id, dmx_ch, value_on, value_off, enable_auto_on, enable_with_blackout, enable_knob, note_text)`
- `content(id, song_id, master_db_id, macro_pattern_id)`: per-track lighting state, keyed to the library track.
- `phrase_data(content_id, phrase_num, macro_id, initial_macro_id)`: per-track phrase to scene assignment.
- `lighting_data(id, content_id, fixture_id, data)`: per-track custom scene XML.
- `lighting_property(key, value)`: key/value settings (for example `ExecVenueId`, `LastSelectedVenue`, `LightingTabNum`, bank/mood/strobe last ids, ambient auto play settings, CDJ lighting flags, user colour, DB version) `LightingDatabaseIF: get/set*Num`.

#### 3.4.4 Fixture library (downloaded, not shipped)

Read through `lighting_db::MasterRepository`. Tables named in SQL strings: `vender_master` (sic), `fixture_master` (has `vender_id`, `fixture_type_id`), `fixture_mode_master` (`fixture_id`, `mode`, `compo_id`, `compo_offset`), `fixture_channel_master` (`mode_id`, `ch_num`), `color_filter_master`, plus wheel and channel-position lists (`findWheelList findChannelPositionList`). Fixture capability queries: `dmx::FixtureData: hasRedCh hasGreenCh hasBlueCh hasDimmerCh hasPanCh hasTiltCh hasStrobeCh hasColorWheelCh hasGoboWheelCh hasColorMacroCh hasRotateMacroCh getChannelModifier getFixtureWheelType`.

Channel function vocabulary in `strings.txt` (near `CustomColorFlagCommand`): `SetRed/Green/Blue/White/...LightLevel`, `SetLightIntensityLevel`, `SetLightHue/Saturation/ValueLevel`, colour temperature and tint, `SetShutterOpen/Closed/Strobe/Duration`, `SetPan/TiltMode/Speed/Direction`, wheel commands (`SetWheelPosition/Speed/SpinDirection/Mode/Frame*`), frost, iris, focus, zoom, framing shutters, fans, lamp on/off, laser on/off/strobe, `SetPreset`, `Reserved`, `RBForcedValue`, and `Custom*Command`. **(inferred)** This is a semantic channel type list per fixture mode, similar in spirit to Open Fixture Library capabilities.

### 3.5 DMX output

- Universe model: fixtures carry `universe_num` and `start_addr`; frames of 512 slots are written per universe by `DMXBuilder::writeToUniverse` through `LightingIOPlugin::writeUniverse`.
- Frame timing: a dedicated thread with a configurable "DMX Frame Frequency" (`strings.txt`); default value not recovered (Q10).
- ENTTEC DMX USB Pro (public Enttec API): FTDI serial at 57,600 baud nominal; host messages are `0x7E, label, length LSB, length MSB, payload, 0xE7`. Label 6 sends a DMX frame (payload = start code 0x00 followed by up to 512 slots). Label 3 reads widget parameters, label 10 reads the serial number. `EnttecDMXUSBPro: configureLine extractSerial writeUniverse`.
- Enttec Open DMX USB: raw FTDI FT232R; the host generates the DMX frame itself: 250,000 baud, 8 data bits, 2 stop bits, no parity, a break of at least 88 us, a mark-after-break of at least 8 us, then start code and slots `EnttecDMXUSBOpen`, `FTD2XXInterface: setBreak setBaudRate setLineProperties`.
- RB-DMX1: AlphaTheta HID DMX interface with authentication (`djplay::HidMapRBDMX1: authSucceeded getDeviceAuth sendData`); frame transport over HID reports; layout unknown (Q8).
- DDJ internal DMX: some controller exposes DMX through HID (`DDJDMXDevice`, `DDJPlugin`, `djplay::DeviceHidDMX`, `updateEnabledDmxIfDdj`); model unknown (Q9).

### 3.6 Video and karaoke formats

- CDG (CD+Graphics): the public format is a stream of 24-byte subcode packets at 300 packets per second; command `0x09` carries graphics instructions: Memory Preset (1), Border Preset (2), Tile Block (6), Scroll Preset (20), Scroll Copy (24), Define Transparent (28), Load Colour Table low/high (30/31), Tile Block XOR (38). Screen 300x216 with a 288x192 visible area, 16-colour palette of 12-bit colours. `rb::CDGFile` method names (`processPacket loadColorTable tileBlock scroll`) match this model. Karaoke files may arrive as a zip (audio + `.cdg`) `rb::CdgZipFileIoStream` **(inferred contents)**.
- Video decoding uses AVFoundation (`AVAssetReader`, `AVAssetReaderTrackOutput`) and Quartz Composer for `.qtz` `imports.txt`.
- Syphon: an Objective-C framework (BSD licence) publishing GL/Metal textures to other macOS apps `Frameworks/Syphon.framework`.

### 3.7 Keyboard mapping files

Stored under a `KeyMappings/` directory (`strings.txt`) as XML with a `KEYMAPPINGS` root **(inferred: JUCE `KeyPressMappingSet::createXml` format, `<KEYMAPPINGS basedOnDefaults="..."><MAPPING commandId="hex" description="..." key="..."/>` and `<UNMAPPING .../>`)**. Each file has a name and timestamp (`KeyMappingFile: getKeyMappingName getTimestamp fileSuffix`). Command ids and defaults are compiled in (Q5).

### 3.8 OSC status monitor

JUCE OSC is linked (`juce::OSCMessage`, `juce::OSCReceiver::Pimpl`, "JUCE OSC server" string). Addresses, port and payload layout for `DJStatusSender` are not in `strings.txt` as plain addresses (Q6).

---

## 4. Device support matrix

Legend: **CSV** = built-in `.midi.csv` file(s); **MidiMap** = dedicated `djplay::MidiMap*` class; **HidMap** = dedicated `djplay::HidMap*` class (`+Sub` = secondary display interface); **Disp** = display builder field map (`D1` = `ControllerDisplayBuilder`, `D2` = `ControllerDisplay2Builder`); **Setup** = Setting Utility framework; **Feat** = notable mapped features measured from the CSV (pads, jog LEDs, stems, video, input selector, MySettings rows). "Name only" means the model name appears in `strings.txt` (alias/device tables) without a mapping file or class.

### 4.1 Controllers (DDJ and all-in-one MIDI)

| Device | CSV | MidiMap | HidMap | Disp | Decks | Feat |
|--------|-----|---------|--------|------|-------|------|
| DDJ-200 | `DDJ-200` | DDJ200 | DDJ200 | | 2 | pads, CFX |
| DDJ-400 | `DDJ-400` | DDJ400 | | | 2 | pads, MySettings |
| DDJ-800 | `DDJ-800` | DDJ800 | | | 4 | pads, input select, MySettings |
| DDJ-1000 | `DDJ-1000` | DDJ1000 | DDJ1000 +Sub | D2 | 4 | pads, on-jog display, input select |
| DDJ-ERGO | `DDJ-ERGO MIDI`, `PIONEER DDJ-ERGO` | DDJERGO | | | 4 | pads |
| DDJ-FLX2 | `DDJ-FLX2` | DDJFLX2 | | | 2 | pads |
| DDJ-FLX4 | `DDJ-FLX4` | DDJFLX4 | | | 2 | pads, MySettings |
| DDJ-FLX6 | `DDJ-FLX6` | (generic) | | | 4 | jog LEDs, Merge FX, MySettings |
| DDJ-FLX10 | `DDJ-FLX10` | DDJFLX10 | DDJFLX10 +Sub | D2 | 4 | stems, Merge FX, drum swap, Mix Point Link, on-jog display |
| DDJ-GRV6 | `DDJ-GRV6` | DDJGRV6 | | | 4 | jog LEDs, stems, drum swap/roll rows |
| DDJ-RB | `DDJ-RB` | DDJRB | DDJRB | | 2 | pads |
| DDJ-REV5 | `DDJ-REV5` | DDJREV5 | | | 4 | jog LEDs, stems, Merge FX |
| DDJ-REV7 | `DDJ-REV7` | DDJREV7 | DDJREV7 +Sub | D2 | 2 | absolute platter, jog timestamp, on-jog display, audio routing behaviour |
| DDJ-RR | `DDJ-RR` | DDJRR | | | 4 | jog LEDs, MySettings |
| DDJ-RX | `DDJ-RX`, `DDJ-RX 2IN2OUT` | DDJRX | | | 4 | jog LEDs, legacy pad class |
| DDJ-RZ | `DDJ-RZ` | DDJRZ | DDJRZ | | 4 | jog LEDs, legacy pad class |
| DDJ-RZX | `DDJ-RZX` | DDJRZX | | | 4 | touch screen X/Y areas, velocity pads |
| DDJ-SB | `DDJ-SB MIDI`, `PIONEER DDJ-SB` | DDJSB | | | 2 | pads |
| DDJ-SB2 | `DDJ-SB2`, `DDJ-SB2 MIDI` | DDJSB2 | | | 4 | pads |
| DDJ-SP1 | `Pioneer DDJ-SP1` | DDJSP1 | | | 4 | pads (sampler/FX controller) |
| DDJ-SR | `PIONEER DDJ-SR` | DDJSR | | | 4 | pads |
| DDJ-SX | `PIONEER DDJ-SX`, `... 2IN2OUT` | DDJSX | | | 4 | jog LEDs, legacy pad class |
| DDJ-SX2 | `PIONEER DDJ-SX2`, `... 2IN2OUT` | DDJSX2 | | | 4 | jog LEDs, legacy pad class |
| DDJ-SX3 | name only (`SX3 2IN2OUT` alias) | | | | | treat as unsupported unless confirmed |
| DDJ-SZ | `DDJ-SZ` | DDJSZ | | | 4 | jog LEDs, legacy pad class |
| DDJ-WeGO | `PIONEER DDJ-WeGO` | WEGO | | | 4 | jog LEDs |
| DDJ-WeGO2 | `PIONEER DDJ-WeGO2` | WEGO | | | 4 | jog LEDs |
| DDJ-WeGO3 | `PIONEER DDJ-WeGO3` | WEGO3 | | | 2 | jog LEDs |
| DDJ-WeGO4 | `DDJ-WeGO4` | DDJWEGO4 | DDJWeGO4 | | 2 | jog LEDs |
| DDJ-XP1 | `DDJ-XP1` | DDJXP1 (+ DDJXMID **(inferred shared)**) | | | 4 | pads (pad controller) |
| DDJ-XP2 | `DDJ-XP2` | DDJXMID **(inferred)** | | | 4 | pads |
| XDJ-AERO | `PIONEER XDJ-AERO`, `... MIDI` | XDJAERO | | | 4 | |
| XDJ-R1 | `PIONEER XDJ-R1` | XDJR1 | | | 4 | input select |
| XDJ-RR | `Pioneer DJ XDJ-RR`, `... MIDI` | (generic) | | | 4 | pads |
| XDJ-RX | `PIONEER XDJ-RX`, `... MIDI` | XDJRX | XDJRX +Sub | D1 | 4 | jog LEDs, screen |
| XDJ-RX2 | `Pioneer DJ XDJ-RX2`, `... MIDI` | XDJRX2 | XDJRX2 +Sub | D2 | 4 | video controls, screen |
| XDJ-RX3 | `XDJ-RX3` | XDJRX3 | XDJRX3 +Sub | D2 | 4 | screen, input select |
| XDJ-XZ | `XDJ-XZ (1)`, `XDJ-XZ (2)` | XDJXZ_1, XDJXZ_2 | XDJXZ +Sub | D2 | 4 | video controls, screen |
| XDJ-AZ | `XDJ-AZ`, `XDJ-AZ MIDI` | XDJAZ | XDJAZ +Sub | D2 | 4 | stems, screens |
| XDJ-AN | `XDJ-AN` | XDJAN | XDJAN +Sub | D2 | 2 | screens |
| OPUS-QUAD | `OPUS-QUAD`, `OPUS-QUAD MIDI` | OPUSQUAD | OPUSQUAD +Sub | D2 | 4 | jog LEDs, screens |
| OMNIS-DUO | `OMNIS-DUO` | OMNISDUO (with takeover) | OMNISDUO +Sub | D2 | 4 | screens |
| PLX-CRSS12 | `PLX-CRSS12` | PLXCRSS12 | | | 4 | DVS turntable with performance pads, stems |

### 4.2 Mixers (DJM and others)

| Device | CSV | MidiMap | HidMap | Setup | Notes |
|--------|-----|---------|--------|-------|-------|
| DJM-250MK2 | `DJM-250MK2` | DJM250MK2 | | DJM-250MK2Setup | input select |
| DJM-450 | `DJM-450` | DJM450 | | DJM450Setup | input select |
| DJM-750MK2 | `DJM-750MK2` | DJM750MK2 | | DJM-750MK2Setup | input select |
| DJM-900NXS2 | `DJM-900NXS2` | DJM900NXS2 | DJM900NXS2 | DJM900NXS2Setup | colour FX matching classes |
| DJM-A9 | `DJM-A9` | DJMA9 | | (PioneerDJMSetup **(inferred)**) | input select |
| DJM-V5 | `DJM-V5` | DJMV5 | | DJM-V5Setup | |
| DJM-V10 | `DJM-V10` | DJMV10 | | DJM-V10Setup | 6 channels, oscillator, audio behaviour, `PDJSetupUtilityDJMV10` |
| DJM-TOUR1 | `DJM-TOUR1` | (generic) | | DJMTOUR1Setup | |
| DJM-S7 | `DJM-S7` | DJMS7 | | DJM-S7Setup | battle mixer with pads, audio behaviour |
| DJM-S9 | `Pioneer DJ DJM-S9` | DJMS9 | | PioneerDJDJMS9Setup | pads, `@DumpDJMS9` |
| DJM-S11 | `DJM-S11` | DJMS11 | DJMS11 +Sub | DJM-S11Setup | screen (D2), pads, oscillator, audio behaviour |
| DJM-T1 | `PIONEER DJM-T1`, `... MIDI` | DJMT1 | | | pads |
| euphonia (rotary mixer) | `euphonia` (declares `EUPHONIA`) | EUPHONIA | | | input select |
| DJM-750, 850, 900nexus, 900SRT, 2000, 2000nexus | name only | | | (PioneerDJMSetup) | audio device names and aliases only |
| INTERFACE 2 | | | INTERFACE2 | | DVS interface, authenticated |

### 4.3 Players (HID control)

| Device | HidMap | Disp | Notes |
|--------|--------|------|-------|
| CDJ-350 | CDJ350 | text builder | |
| CDJ-400 | CDJ400 | D1 | |
| CDJ-850 | CDJ850 | D1 + text builder | |
| CDJ-900 | CDJ900 | D1 | |
| CDJ-900NXS | CDJ900NXS | D1 | |
| CDJ-2000 | CDJ2000 | D1 | |
| CDJ-2000NXS (nexus) | (name only; likely shares CDJ2000 **(inferred)**) | | |
| CDJ-2000NXS2 | CDJ2000NXS2 | D2 (and v1.30 variant) | |
| CDJ-3000 | CDJ3000 | D2 | KUVO metadata hooks |
| CDJ-3000X | CDJ3000X | D2 | |
| CDJ-1500X | CDJ1500X | D2 | |
| CDJ-TOUR1 | name only | | |
| XDJ-1000 | XDJ1000 | D1 | |
| XDJ-1000MK2 | XDJ1000MK2 | D2 | |
| XDJ-700 | name only | | |
| Networked players (LAN HID) | LANHID | | Pro DJ Link connected players in HID mode **(inferred)** |

### 4.4 Other hardware

| Device | Interface | Evidence |
|--------|-----------|----------|
| RB-DMX1 | HID DMX, authenticated | `djplay::HidMapRBDMX1`, `strings.txt: RB-DMX1` |
| ENTTEC DMX USB Pro, Enttec Open DMX USB, DMXKing ultraDMX, FTDI USB COM485 Plus1, Vince USB-DMX512 | FTDI D2XX | `EnttecDMXUSBPro`, `EnttecDMXUSBOpen`, `DMXUSB` |
| DDJ with internal DMX (model unknown) | HID | `DDJDMXDevice`, `strings.txt: DDJ-InternalDMX` |
| Generic HID DMX | HID | `HIDDMXDevice`, `HIDPlugin`, `djplay::HidMapDMX` |
| KUVO gateway (NXS-GW) | HID | `djplay::HidMapKUVO` |
| Phase Essential, Phase Pro, Phase Pro Receiver | HID (+ MIDI for Pro) | `HidMapPhaseEssential`, `HidMapPhasePro`, `MidiMapPhasePro` |
| Generic MIDI controllers | MIDI Learn only | `strings.txt: You can map each function with MIDI LEARN even on MIDI equipment other than HardwareUnlock devices.` |
| USB/built-in cameras | AVFoundation | `rb::CameraDevice` |

Totals: 69 mapping files covering 55 distinct hardware models (after merging port and alias variants); 53 `MidiMap*` subclasses; 50 `HidMap*` classes (36 base classes, including the non-player `DMX`, `KUVO`, `LANHID`, `INTERFACE2` and `RBDMX1`, plus 12 `Sub` display helpers and 2 Phase `Deck` helpers).

---

## 5. Internal architecture as observed

Names in this section are rekordbox's. Relationships are inferred from member names and namespaces unless a symbol shows them directly.

```
CoreMIDI ──> djplay::DeviceMidi ─┐                      ┌─> djplay::DeviceConnector ─> DeviceUnit/DeviceComponent ─> app (players, mixer, FX, browse, video, lighting)
IOHIDManager ─> djplay::DeviceHid ┤ djplay::DeviceManager┤
LAN HID ───────────────────────────┘ (singleton, listeners)└─< operate*Change (feedback) <─ app state, DisplayDataManager
            │                                                          │
            └─ MidiMap / HidMap (per device codec, CSV-driven for MIDI) ┘
```

- `djplay::DeviceManager` (singleton) owns device lifetimes: connect/disconnect/enable for MIDI, HID and LAN HID, MIDI Learn start/stop, alias resolution, dongle (Hardware Unlock) tracking, listener registration `DeviceManager: singletonHolder connectMidiDevice connectHidDevice connectLanHidDevice startMidiLearn registListener`.
- `djplay::DeviceMidi` and `djplay::DeviceHid` are per-device runtimes (threads and timers): receive on the device thread (`midiInDataForThread`, `handleIncomingHidMessage`), dispatch to the message thread with `handleAsyncUpdate`, and send feedback via `operateOnChange operateValueChange operateLongValueChange operateBlinkChange operateSetProperty`.
- `djplay::MidiMap` is the CSV-backed codec: `load`, `loadCsvLine`, `analyzeCsvLineV1`, `analyzeCsvOptionV1`, input/output tables (`setMidiInputMap setMidiOutputMap getMidiInputData getMidiOutputData`), `channelMask` (deck channel offsets), `exportTo importFrom createCsv createCsvLine` (round trip for MIDI Learn), SysEx (`addSysEx getSysExLimit getSysMessage`) and authentication hooks. Subclasses override per-device behaviour.
- Element objects (`MidiButton`, `MidiKnobSlider`, `MidiJogRotate` and so on, each with `handleInputMidiData` and `operateSetProperty`) convert raw messages into typed events.
- `djplay::DeviceConnector` is the event bus between devices and app components: `eventOnChanged eventValueChanged eventRelativeValueChanged eventAbsoluteValueChanged eventCountChanged eventPropertyChanged`, with takeover support and component/unit registration (`addComponent addUnit`). App components implement `DeviceComponent`/`DeviceUnit` (for example `ChannelLevelDeviceUnit`, `SamplerPadDeviceComponent`, `effectGui::RbxCFXDeviceComponent`, `PadDeviceUnit`), and properties travel as `djplay::DeviceVariant`.
- Jog: raw counts go to `djplay::JogProcessor`/`JogValue` and then into `DjController::*` processors that run on the audio callback (`AudioCallbackTimeStampProcessor5` has 69 members), so scratch is sample-accurate. `rbxfrm::ControllerPerformanceBehavior` hooks pre-command and post-render tasks.
- Feedback output: `MidiOutputHelper` (background thread, priority queue, `MidiOutInterval` pacing) and `PdjUsbHidOutputHelper` (HID, priority, buffer accounting). `BlinkManager` drives blinking.
- Displays: `djplay::DisplayDataManager` is the model of everything a hardware screen shows (154 members, with listeners). `ControllerDisplayBuilder`/`ControllerDisplay2Builder` serialise it into per-model field maps for HID `outputUnit`.
- Mixer hardware: `DeviceDefs` capability table, `AudioPrefBehavior*` and `AudioOutputChannelBehavior*` routing rules, `nonframework_pdjfunc` and the ObjC `PDJFunc` notifier bridging the Setting Utility frameworks (`PDJ_*` C API loaded from `*Setup.framework`).
- MIDI Learn: `djplay::MidiLearnComponent` (window) holds `MidiLearnTableComponent` per tab, seeded from embedded XML (`MidiLearnBinaryData::midi_command_data_xml`, `midi_learn_table_data_xml`), and edits a `MidiMap` via `setDeviceMidiMap`, `DeviceMidi::startMidiLearn`.
- Keyboard: JUCE `ApplicationCommandManager` pattern. Each command target (`VideoPanel`, `dmx::LightingKeyboardShortcut`, `GrooveCircuitKeyboardShortcut`, `AbletonLinkKeyboardShortcut`, main app) exposes `getAllCommands getCommandInfo perform getNextCommandTarget`; `KeyMappingFileManager` persists mappings; `rb::KeyMappingEditorComponent` edits them.
- Lighting stack (source paths in `strings.txt`: `Source/Middleware/DJDevice/lighting/{engine,interfaces,dmxusb,hid,ddj}`, `Modules/LightingDB/Source/LightingDB/{Core,DataAccess}`, `Source/App/GUI/Panel/Lighting/{,DMX}`):
  - `lighting_db::LightingDatabaseIF` (singleton facade, 130 members) over `MacroRepository` (macro.db3), `UserRepository` (user.db3), `MasterRepository` (fixture library), all `SQLRepository`.
  - `dmx::LightingDataLoader` (thread) loads per-track data on deck load and from CDJs; `SongStructureBasedLightingData`, `MacroLightingData`, `SongBasedLightingData`, `FixtureBasedLightingData` evaluate brightness/colour/strobe/pan-tilt/rotate/gobo at a beat position; `dmx::LightingClockGenerator` provides beat time.
  - `dmx::DMXBuilder` (thread, 194 members) combines evaluated scenes, live overrides (mood/bank/colour/strobe/blackout/direct control), fixture channel layouts (`dmx::FixtureData`) and pan/tilt limits (`dmx::PanTilt`) into DMX frames, then writes them through `LightingDeviceManager` to `LightingIOPlugin` implementations (`DMXUSB`, `HIDPlugin`, `DDJPlugin`).
  - `dmx::DMXBuildConfiguration` (singleton) is the control surface used by UI and MIDI.
  - The plugin layer (`LightingIOPlugin`, `DMXUSB`, `DMXUSBWidget`, `EnttecDMXUSBPro`, `EnttecDMXUSBOpen`, `FTD2XXInterface`, `HIDPlugin`, `HIDDMXDevice`, `DMXInterface`) has the class names, method names and a user-facing string of QLC+'s `dmxusb` and `hid` plugins **(inferred: derived from QLC+, Apache-2.0)**.
  - UI: `djplay::LightingPanel` tabs (Fixture Library, Macro Mapping, Macro Editor, CDJ Lighting), `LightingControlPanel`, `LightingBankSelectorView`, `WidgetLightingPlayer` (mini player inside lighting mode), look-and-feel classes (`FixtureLibraryLookAndFeel`, `LightingLookAndFeel`, `MacroEditLookAndFeel`, `MacroMappingLookAndFeel`).
- Video stack: `VideoPanel` (UI, command target, GL renderer for transitions `renderTFX`, effects `renderVFX`, overlays) with `VideoPanel::Setting` (singleton settings), `videoPanel::MasterOutComp` (output window with shared GL context), `rb::MFVideoReader` (decoder thread with cue buffers per deck), `rb::CameraDevice`, `rb::CDGFile`, `rb::ThumbnailGenerator`, `VideoOutput`, and `ExtensionSyphonOut` toggling a `SyphonServer`. `VideoPanelSS` is the separate-shader variant.
- OSC monitor: `djplay::djmonitor::DJMonitor` (receiver, account-aware), `DJListenersController` (listener registry with expiry and request validation), `DJStatusSender` (thread that samples player/mixer state through `DJPlayerAccess` and sends OSC), `LogController` (CSV log to analytics).

---

## 6. Prior art and licences

| Item | Licence | Use in Cuepoint |
|------|---------|-----------------|
| AlphaTheta `.midi.csv` mappings | Proprietary, shipped inside rekordbox | **Do not ship or copy.** The repository rule already forbids shipping mapping files (`docs/spec/README.md`). MIDI message numbers are facts and can be re-derived from AlphaTheta's public "MIDI Message List" PDFs for each controller. Plan: author our own mapping files in our own schema from the public PDFs and our own captures, and provide an importer that reads a user-supplied rekordbox `.midi.csv` (from the user's own rekordbox install or MIDI Learn export) at runtime. |
| AlphaTheta `LIGHTING_DB.zip` macros and downloaded fixture library | Proprietary content | Do not ship. Author our own preset scenes in the same model (mood x bank x phrase); optionally import from a user's own rekordbox installation. For fixtures, use Open Fixture Library. |
| Pad definition CSVs (`Res/pad`) | Proprietary | Do not ship; the mode list is a feature fact, re-author in our format. |
| Mixxx controller mappings and docs | GPL-2.0-or-later | Reference for behaviour only. Do not copy mapping XML/JS into this MIT/Apache workspace. |
| QLC+ (`dmxusb`, `hid` plugins, fixture definitions) | Apache-2.0 | Compatible. rekordbox's DMX plugin layer appears to derive from it. We may port logic with attribution in a NOTICE file, but the Rust plan below avoids D2XX entirely. |
| Open Fixture Library | MIT (data and code) | Candidate fixture library source; convert to our fixture schema at build time. |
| Enttec DMX USB Pro API document | Public specification | Implement from the document. |
| CDG format | Public (Red Book subcode, widely documented) | Implement our own decoder. |
| Syphon framework | BSD-2-Clause | Can be bundled and called through Objective-C bindings on macOS. |
| FTDI D2XX (`libftd2xx`) | Proprietary freeware licence | Avoid. Use the OS serial driver (macOS ships AppleUSBFTDI) or a pure-Rust FTDI implementation over raw USB. |
| Ableton Link | GPL-2.0-or-later or commercial licence | Engine spec owns the decision; hardware layer only exposes Link commands. Flag that any Link binding has licence implications for distribution. |
| Pioneer HID protocols, Hardware Unlock authentication (`DeviceAuth`) | Proprietary, access control | Do not reimplement or bypass authentication (repository rule: "DRM and licence checks are not circumvented"). Cuepoint has no paid tiers, so Hardware Unlock is unnecessary. If a device refuses to operate without authentication (Q7), that device is out of scope for HID; MIDI mode remains. |
| AlphaTheta keyboard presets | Proprietary layout facts | Default shortcut tables may be reproduced as functional facts (key to command), written by us; prefer our own defaults plus an optional "rekordbox layout" preset authored from the public manual. |

Rust ecosystem (licences as published by each crate; verify at adoption time):

- MIDI: `midir` (MIT; CoreMIDI, ALSA, WinMM, virtual ports on macOS/Linux), `coremidi` (MIT) if we need MIDI 2.0 event lists or unique ids, `wmidi` or `midi-msg` for message types.
- HID: `hidapi` wraps the C hidapi library (not pure Rust). Pure-Rust options: `async-hid` (uses platform APIs directly), or our own IOHIDManager binding via `objc2-io-kit`/`io-kit-sys` on macOS. `nusb` (pure Rust USB) cannot claim interfaces owned by the macOS HID driver, so it suits vendor-class interfaces only.
- Serial/FTDI: `serialport` for Enttec Pro over the VCP driver; pure-Rust FTDI control requests over `nusb` for Open DMX if the VCP break timing is not precise enough.
- DMX over network (not required for parity, cheap to add): `artnet_protocol`, `sacn`.
- OSC: `rosc`.
- Video: no mature pure-Rust H.264/HEVC decoder. Use platform decoders (`objc2-av-foundation` on macOS, Media Foundation on Windows) behind a trait; render with `wgpu` (shared with egui via `eframe`). Camera: `nokhwa` or AVFoundation bindings.
- Syphon: hand-written `objc2` bindings to `Syphon.framework`.
- XML: `quick-xml`. SQLite: whatever the library spec chooses (`rusqlite` bundles C SQLite; reading `macro.db3` and `user.db3` needs SQLite either way).

---

## 7. Open questions for Ghidra

Each names the symbols to decompile. Purpose is behaviour description only; no code is to be transcribed.

1. **CSV parsing rules.** `djplay::MidiMap::analyzeCsvLineV1`, `djplay::MidiMap::analyzeCsvOptionV1`, `djplay::MidiMap::loadCsvLine`, `djplay::MidiMap::channelMask`, `djplay::MidiMap::getSwitchType`, `djplay::MidiMap::getCharValue`. Confirm: deck column semantics, handling of `RO`, `Fast`, `Dual`, `Priority`, unknown options, duplicate names, the `function` column's role, and whether `name.Variant` maps to a value.
2. **Element value semantics.** `djplay::MidiRotary::handleInputMidiData`, `MidiRotaryAsKnob::handleInputMidiData` (meaning of `Value=2048`), `MidiDifference`, `MidiJogRotate`, `MidiJogRotateAbsolutePosition`, `MidiJogRotateTimeStamp`, `MidiKnobSlider` (14-bit LSB pairing, which arrives first), `MidiVariableMsb/Lsb`, `MidiSx2TrimVariableLsb`, `MidiValue` (Min/Max), `MidiIndicatorLong`, `MidiJogIndicatorLong`.
3. **Jog to transport maths.** `DjController::ScratchProcessor3`, `DjController::JogCountSmoother`, `DjController::AudioCallbackTimeStampProcessor5`, `DjController::SearchProcessorSpeed`, `DjController::Rpm`, `djplay::JogProcessor::operateSetProperty`: ticks per revolution per device, smoothing constants, vinyl brake/release curves (`rbxfrm::VinylSpeedAdjustor`).
4. **Device table.** `djplay::DeviceDefs::internalGetDeviceInfo`, `djplay::DeviceDefs::s_deviceInfo`, `DeviceDefs::matchDeviceName`, `DeviceDefs::retrieveDeviceName`, `DeviceManager::getRealNameFromAlias`: vendor/product ids, capability flags, name matching order.
5. **Keyboard defaults.** Default `KeyPress` per command in each `getCommandInfo` (`VideoPanel::getCommandInfo`, `dmx::LightingKeyboardShortcut::getCommandInfo`, main application command target), `KeyMappingFile::defaultKeyMappingName`, `_kPerformaceKeyMappingDefaultValue`, `KeyMappingFileManager::isPresetSerato`, `KeyMappingFile::fileSuffix`, `rb::KeyMappingEditorComponent::needModifier`.
6. **OSC monitor protocol.** `djplay::djmonitor::DJStatusSender::sendOSCMessage`, `DJStatusSender::updatePlayerStatus`, `updateMixerStatus`, `DJListenersController::judgementOscRequest`, `DJListenersController::addOscListener`, `DJMonitor::oscMessageReceived`: OSC addresses, ports, payload types, listener expiry, and what `codecCertificationKey` gates.
7. **Authentication dependence (not implementation).** `djplay::DeviceAuth::isSupportedDevice`, `DeviceDefs::isHidOpenAfterMidiAuth`, `DeviceDefs::isPioneerDongleDevice`, `djplay::HidMap::inputUnitAuth`: only to learn which devices withhold normal MIDI/HID operation until authenticated, so we can document them as limited. Do not record keys, tables or algorithms.
8. **RB-DMX1 frame transport.** `djplay::HidMapRBDMX1::sendData`, `HidMapRBDMX1::outputUnit`, `djplay::DMXDataBuilder::getNextExtent`, `DMXDataBuilder::setData`.
9. **Internal DMX on a DDJ.** `DDJDMXDevice::outputDMX`, `DDJPlugin::rescanDevices`, `dmx::DMXBuilder::updateEnabledDmxIfDdj`: which model and which transport.
10. **DMX timing and channel mapping.** `dmx::DMXBuilder::run`, `dmx::DMXBuilder::writeToUniverse`, `dmx::FixtureData::getChannelModifier`, `dmx::FixtureData::getFixtureChannelRange`, `dmx::PanTilt::getPan`, `dmx::PanTilt::getTilt`, `EnttecDMXUSBOpen::run`: frame rate default, how colour is mapped to RGB vs colour wheels vs colour macros, pan/tilt scaling with offsets/limits, dimmer and strobe mapping, lighting delay compensation.
11. **Scene evaluation.** `dmx::SongStructureBasedLightingData::parseBrightness`, `parseColour`, `parseStrobe`, `parsePosition`, `parseRotate`, `parseFillIn`, `dmx::LightingDataLoader::loadFillInLightingDataByContentId`, `dmx::MacroLightingDataLoader`: interpolation rules, `Point.type` meaning, `MovementBlock` maths, what "fill in" means, how phrases shorter or longer than 32 beats are handled.
12. **Gobo XML.** `CLCEGoboBlock`, `dmx::SongStructureBasedLightingData::parseGobo`: attribute names.
13. **Fixture library file.** `djplay::FixturesDataManager::parseFixturesFile`, `FixturesDataManager::downloadFixtureLibrary`, `FixturesDataManager::fixtureLibraryUrl`, `lighting_db::MasterRepository::findFixtureModeInfoListByCompoId`: download format, master DB schema columns, meaning of `compo_id`.
14. **Auto deck selection for lighting.** `dmx::DMXBuilder::getActiveDeckNo`, `DMXBuilder::isPossibleAutoSelection`, `djplay::CDJLightingLinkManager::getActiveDeckNo`.
15. **Video maths.** `VideoPanel::renderTFX`, `VideoPanel::renderVFX`, `VideoPanel::calGrid2Param`, `VideoPanel::getBrightnessByPlayStatus`, `VideoPanel::VideoCrossFaderProcessor`, `VideoPanel::startAutoTransition`: parameter mapping from crossfader to transition progress, beat sync of VFX, brightness curve from channel fader.
16. **CDG specifics.** `rb::CDGFile::processPacket`, `rb::CdgZipFileIoStream`: confirm the standard instruction set and the zip layout accepted (file naming of audio + `.cdg`).
17. **MIDI output pacing.** `djplay::MidiOutputHelper::run`, `MidiOutputHelper::sendPriorityMessage`, `djplay::PdjUsbHidOutputHelper::getPriority`: queue discipline and the unit of `MidiOutInterval`.
18. **Per-device SysEx feedback.** `djplay::MidiMapDDJFLX10::makeBeatInfo`, `makeEffectsInfo`, `getBpmInfoMessage`, `djplay::MidiSysExVer::getSysExMessage`, `MidiSysExSer::getSysExMessage`: message layouts (compare against public MIDI message lists first).
19. **Double click and long press timing.** `djplay::MidiMap::DoubleClick`, `djplay::BlinkManager`.
20. **Lyrics source.** Callers of `common::kLyricAcquired` and `VideoPanel::onLyricPanelVisibilityChanged`: which service supplies lyrics (may be a webview feature) and whether it belongs to this subsystem.

---

## 8. Rust plan

### 8.1 Crates and modules

The workspace already has `cuepoint-control`, `cuepoint-lighting` and `cuepoint-video`. The workspace denies `unsafe_code`; platform FFI (IOHIDManager, Syphon, AVFoundation) therefore goes into a small dedicated crate, for example `cuepoint-platform-macos`, with a crate-level `allow(unsafe_code)` and a narrow safe API.

`cuepoint-control`:

- `transport::midi`: port enumeration, hot plug, open/close, send queue with priority and pacing (`midir`).
- `transport::hid`: trait `HidTransport` with a macOS IOHIDManager backend (via the platform crate) and a test backend.
- `transport::lan_hid`: later, alongside `cuepoint-prolink`.
- `device`: our own device registry (name patterns, aliases, port roles, capabilities), authored by us as data (TOML or RON).
- `mapping::model`: our mapping schema: controls (type, message, channel/deck binding, range, encoding), bindings (control + modifier layer to command), feedback outputs (LED, ring, value, blink, priority).
- `mapping::rekordbox_csv`: importer for user-supplied rekordbox `.midi.csv` files implementing section 3.1 exactly (BOM, CRLF, comments, separators, 15-column rows, deck offsets, options), producing `mapping::model`. Exporter for round trips.
- `mapping::vocab`: our command enum mirroring the 1,269 CSV names and 711 MIDI Learn commands, with a stable string id table for import.
- `elements`: decoders for each element type (button, pad, 7-bit, 14-bit, relative encoder, rotary-as-knob, jog delta, absolute platter, jog timestamp, value, raw value, parameter), each a pure function from message to typed event.
- `jog`: jog processing (scratch, bend, search, vinyl mode, jog cutter, brake/release), producing transport commands for the engine; DSP-rate parts run in `cuepoint-engine` through a lock-free queue.
- `feedback`: LED state cache, blink scheduler, priority queue, takeover state.
- `display`: a `DisplayModel` (what a screen shows) and per-device serialisers; HID screens only where we can legally capture and describe protocols.
- `learn`: MIDI Learn session (capture next message, propose type, conflict detection).
- `keyboard`: command registry, key binding sets, presets, import of JUCE `KEYMAPPINGS` XML, egui key event adapter.
- `osc_monitor`: optional `rosc` status sender once Q6 is answered.

`cuepoint-lighting`:

- `db`: readers for rekordbox `macro.db3` and `user.db3` (import only), and our own store.
- `scene`: scene XML model (`LightingEditModel`) parse/serialise with tolerant floats; our own preset scene set.
- `fixtures`: fixture schema (channels with semantic types), Open Fixture Library converter, venue and patch model (universe, address, mode, pan/tilt offsets and limits, tilt reversal).
- `engine`: beat clock, phrase to scene resolution (mood x bank x phrase slot), evaluation of brightness/colour/strobe/position/rotate/gobo at a beat, live overrides, direct control, deck selection, delay compensation, DMX frame assembly.
- `output`: `DmxOutput` trait with backends `enttec_pro` (serial), `open_dmx` (serial with break or raw FTDI via `nusb`), `artnet`, `sacn`, `null`/`capture` for tests. RB-DMX1 only if Q8 can be answered without authentication.

`cuepoint-video`:

- `decode`: `VideoSource` trait; platform backends; image sources; CDG decoder (pure Rust).
- `compose`: `wgpu` compositor with two decks, crossfader, brightness, transitions (ids 10-38), VFX (50-73), colour adjust (150-155), overlays (text, image, camera), touch FX routing.
- `output`: output window (second screen, full screen), Syphon (macOS, platform crate).
- `camera`: capture backend.
- `thumbs`: thumbnail generation for the browser.

### 8.2 Licensing decisions to record

- Ship no AlphaTheta CSVs, pad files, macro DBs or fixture data. Ship our own mappings for devices we own or can capture, built from public MIDI message lists.
- Provide "Import rekordbox mapping" (file picker or auto-detect a local rekordbox install) so users can bring their own files.
- No D2XX; no authentication code.

### 8.3 Parity test strategy

1. **CSV conformance.** A test that, when `/opt/rekordbox/7.2.19/payload/.../MidiMappings` exists (skipped in CI otherwise), imports all 69 files and asserts: row counts per type and option match the figures in section 3.1, every deck binding resolves to a valid message, export then re-import is lossless. Synthetic fixtures written by us cover every rule in the repository.
2. **MIDI replay.** Record real controller sessions (our hardware) as timestamped message logs. Replay them through `cuepoint-control` with a fake engine and assert the emitted command stream and the LED/feedback stream. Where rekordbox is available on a test Mac, record rekordbox's MIDI output for the same input (CoreMIDI virtual loopback or a USB MIDI monitor) and diff feedback streams.
3. **Virtual devices.** Use `midir` virtual ports (macOS CoreMIDI, Linux ALSA) to impersonate a controller by name, so device matching, alias handling and hot plug are tested end to end without hardware. A scripted "device simulator" plays both directions.
4. **HID replay.** Capture USB traffic from our own devices with rekordbox and Cuepoint (macOS USB packet capture or a hardware analyser), store reports as fixtures, and assert our codecs produce identical display and LED reports for the same app state.
5. **Jog numerics.** Feed synthetic tick trains (constant spin, stop, reverse, touch/release) and assert position and speed curves against curves measured from rekordbox audio output (record rekordbox's master output while replaying the same MIDI, then cross-correlate playback position).
6. **Lighting golden frames.** For a fixed track, venue and phrase set, capture rekordbox's DMX output with an Enttec Pro in receive mode (or a second interface). Assert our frames per beat match within a small tolerance after aligning on the beat clock. Unit tests evaluate each scene XML element type at chosen beats.
7. **DB import.** Import `macro.db3` from the evidence path (skipped when absent) and assert 183 macros, 4,520 data rows, 27 patterns and 232 assignments, and that every XML parses.
8. **Video golden images.** Render each transition and VFX at fixed progress values and parameters, compare with frames grabbed from rekordbox's Syphon output using SSIM thresholds. CDG decoder tested against reference renders from public test discs we create ourselves.
9. **Keyboard.** Import a JUCE `KEYMAPPINGS` XML produced by rekordbox's export, verify round trip and conflict detection.
