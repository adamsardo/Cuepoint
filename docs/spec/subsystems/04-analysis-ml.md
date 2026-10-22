# 04. Track analysis and machine learning

Status: draft from static evidence only (symbols, strings, class tables, bundled model files). No decompilation has been done for this section yet. Text marked **[inferred]** is our reading of the evidence, not a confirmed fact. Text marked **[verify]** needs checking against a running rekordbox or a Ghidra pass (section 6).

Evidence locations (outside the repo): `analysis/classes.tsv`, `analysis/syms.demangled.txt`, `analysis/strings.txt`, `analysis/imports.txt`, and the bundle at `rekordbox.app/Contents/{Resources,Frameworks}`. In the backticked evidence below, `cls:` is a scope in classes.tsv, `sym:` is a demangled symbol, `str:` is a string, `file:` is a bundled file and `dylib:` is an exported symbol of a framework dylib.

## 1. Purpose

This subsystem turns an audio file into everything rekordbox knows about the track that does not come from tags:

- tempo, beat grid (constant or dynamic), downbeat and bar numbering;
- musical key;
- waveforms in every flavour the UI and CDJ/XDJ hardware use (mono, colour, 3-band; preview and detail);
- phrase / song structure (intro, verse, chorus, up, down, outro and so on, with mood);
- vocal position;
- loudness, sample peak and the derived Auto Gain value;
- "Intelligent Cue" suggestions (ML cue point prediction, personalised to the user's own cue habits);
- audio embeddings, energy and genre used by Track Suggestion, Collection Radar and Streaming Radar;
- cloud analysis (search and download pre-computed ANLZ data, upload own results);
- stem separation (Track Separation / STEMS), which shares the ML runtime stack.

It also owns writing the results into the ANLZ files (`ANLZnnnn.DAT`, `.EXT`, `.2EX`) through `analyzer::CMusicAnalyzeFile`, and the editing tools that change the results afterwards (`djplay::BeatGridAdjustment`, phrase editing).

The ANLZ binary layout itself is specified with the export / device-library subsystem; this section lists which tags analysis produces and in which order.

## 2. Feature inventory

### 2.1 Analysis orchestration and settings

- [ ] Single analysis facade (singleton) that starts, pauses, cancels and reports analysis for one or many tracks, and answers "is anything analysing". `cls:analyzer::CAnalyzerIF` (start_analyze, pause_analyze, cancel_analyze, isAnalyzing, isAnalyzingAnyTracks, isRunningBeatOrWaveAnalyze, singletonHolder)
- [ ] Worker thread per analysis job with start / stop / pause / restart, progress notification and interruption. `cls:analyzer::CBeatAnalyzeThread` (startAnalyze, restartAnalyze, pauseAnalyze, stopAnalyze, Interupt, notifyProgress, s_id)
- [ ] Job graph: a controller runs a set of jobs (cloud search, cloud download, beat detect, key, and further stages) with per-job triggers, special info and last status. `sym:analyzer::AnalyzeJobController::SetJobs(analyzer::AnalyzeJobs)`, `SetTrigger(AnalyzeJobs,bool)`, `SetLastStatus(AnalyzeJobStatus, juce::String, int)`
- [ ] Per-run job selection flags: Beat, BeatUnit, BpmValue, Embedding, Key, NetAnalyze (cloud search), NetDownload, Phrase, Urahaku (off-beat correction, **[inferred]** from Japanese 裏拍), Vocal. `cls:analyzer::AnalyzeJobManager`
- [ ] Parameter manager holding user settings, each change tagged with a reason: Beat on/off, BeatAnalyzeMode, BPM range, Cue analysis mode, Key on/off, NetAnalyze, NetDownload, Phrase, Vocal. `sym:analyzer::AnalyzeParametorManager::SetBpmRange(int, AnalyzeParametorManagerReason)` and siblings
- [ ] Preferences > Analysis > Track Analysis: Analysis Mode **Normal** (constant tempo) / **Dynamic** (tempo changes) / **Auto** (only with high precision; picks per track). `str:Track Analysis Mode`, `str:Dynamic`, `str:For tracks with a constant tempo`, `str:For tracks that change tempo`, `str:When selecting auto while using high precision BeatGrid analysis,`
- [ ] BPM Range presets: 58-115, 68-135, 70-180, 78-155, 88-175, 98-195, 108-215, 118-235, 128-255. `str:70-180|58-115|68-135|...|128-255`, `str:_bpmRange`
- [ ] "Use high precision BeatGrid analysis" option, with stated cost +90% to +270% time. `str:HighPrecisionBeatAnalyze`, `str:Analysis time increase rate compared to the conventional BPM/Grid analysis`, `cls:AnalysisTrackAnalysisHighPrecisionBeatComponent`
- [ ] Track Analysis Setting checkboxes: BPM / Grid, Key, Phrase, Vocal, Embedding (Radar Analysis). `str:Track Analysis Setting`, `str:BPM / Grid`, `str:Phrase`, `str:TrackAnalysisSettingsEmbedding`, `cls:AnalysisTrackAnalysisEmbeddingAnalysisComponent`
- [ ] Auto Analysis on import (BPM/grid always analysed on import / load / export if missing). `str:Auto Analysis is starting.`, `str:If tracks are not analyzed, BPM/Grid analysis starts when importing, loading, or exporting the tracks`, `cls:AnalysisTrackAnalysisAutoAnalysisComponent`
- [ ] Collection-wide analysis: start / pause / stop / load / analyse missing. `cls:db::DatabaseIF` (analyzeCollectionStart, analyzeCollectionPause, analyzeCollectionStop, analyzeCollectionLoad, analyzeCollectionMissing, getAnalyzingTracks)
- [ ] Analysis Lock per track: blocks Track Analysis, Key Analysis and grid editing. `str:Track Analysis, Key Analysis, and Beatgrid Adjustment are locked.`, `str:Beatgrid adjustment error. Turn off Analysis Lock.`, `sym:db::DatabaseIF::analysisLock`, `cls:browse::Status` (isAnalysisLocked)
- [ ] Status flags shown in the browser: analysing, analysed with old engine, phrase / vocal / embedding analysed, combined icons. `cls:browse::Status` (isAnalysing, isAnalyzedOldVersion, hasAnalyzedSongStructure, hasAnalyzedVocalDetect, hasAnalyzedEmbeddingData), `str:Phrase and Vocal and Embedding analyzed`, `str:brows_phrase_vocal_embedding_icon.svg`
- [ ] Analysis engine version recorded per track and compared for "old version" re-analysis. `sym:analyzer::CAnalyzerIF::GetAnalysisEngineVersion`, `cls:analyzer::BALibVersion`
- [ ] Licence / plan gating of ML features (Intelligent Cue, vocal detection, cloud analysis download). `cls:ActivationComponent` (canUseCueAnalisysFunction, canUseCloudAnalysisDownloadFunction), `cls:common::RbAppBackdoor` (canAnalyzeIntelligentCue, canAnalyzeVocalDetection, canCloudAnalysisDownload)
- [ ] Removal of stale analysis files; relocation of audio path inside an ANLZ file. `sym:CAnalyzerIF::removeOldAnalysisFiles`, `modifyAudioPathInAnalyzeFile`
- [ ] Input decoding with seek tables: FLAC seek table built at analysis time; MP3 frame offsets for VBR index. `cls:analyzer::FlacAnalyzeManager` (createSeekTable, getSeekPointAtTime), `cls:analyzer::MpegFrameOffset`, `sym:CBeatAnalyzeThread::createSeekTable`, `sym:CAnalyzerIF::notify_seek_table`
- [ ] Streaming tracks analysed too, with provider, track id and audio quality at analysis time recorded. `sym:CAnalyzerIF::GetStreamingProvider`, `GetStreamingTrackId`, `GetStreamingAudioQualityWhenAnalyzed`, `sym:db::DatabaseIF::getStreamingAnalyzeCollection`

### 2.2 Beat, BPM and beat grid

- [ ] Two legacy DSP beat engines selectable by version: BeatAnalyzer 1.0 and 2.0, each with 44.1 kHz and 48 kHz variants, and "Const" vs "Varied" (constant vs dynamic tempo) constructors. `cls:BeatAnalyzer_1_0` (BA_CreateConst44k, BA_CreateConst48k, BA_CreateVaried44k, BA_CreateVaried48k), `cls:BeatAnalyzer_2_0`, `sym:analyzer::BALibWrapper::BALibWrapper(analyzer::BALibVersion)`
- [ ] Legacy engine stages: resample, IIR band filters (low / mid / high / band pass), envelope makers, multi-band beat sensor with band selection, peak buffer, beat list maker, beat info analyser with gap correction and segment adjustment, BPM and beat position adjustment, beat cutting. `cls:BeatAnalyzer_2_0::IirPassFilter`, `::UnitEnvelopeMaker`, `::peakss::BeatSensor` (selectBand), `::peakss::BeatInfoAnalyzer` (gapCorrect, segAdjust, judgeHendouDynamicPeak), `::UnitBeatAdjust` (bpmAdjust, beatPositionAdjust, offsetBeatPosition, getPeakBpm)
- [ ] Engine 2.0 additions: "Focus" beat positions (refined local positions), BPM fluctuation ("Hendou", 変動) enable flag for dynamic grids, offset calculation. `sym:BALibWrapper::CalcBeatPos_Focus`, `GetBpm_Focus`, `SetBpmHendou(bool)`, `GetBpmHendou`, `GetOffsetCalc`
- [ ] High precision beat analysis using a neural beat / downbeat tracker (TensorFlow SavedModel, section 3), selected when the option is on. `sym:analyzer::AnalyzeJobBeatDetector::highPrecisionBeatAnalyze()`, `legacyBeatAnalyze()`, `file:Resources/models/detect_beat/model7`, `model8`
- [ ] Fit detected BPM into the chosen BPM range by doubling or halving. `sym:AnalyzeJobBeatDetector::bpmToDouble`, `bpmToHalf`
- [ ] First beat / first bar detection and beat renumbering so beat 1 lands on a downbeat. `sym:AnalyzeJobBeatDetector::getFirstBeatPos()::convFirstBar`, `renumberBeat(..., int, long long)`, `sym:CBeatAnalyzeThread::analyze()::convFirstBar`
- [ ] Beat times converted to microseconds / milliseconds and stored as a "detailed" beat list (per-beat BPM, beat number within bar, time). `sym:AnalyzeJobBeatDetector::convertToMicrosec`, `cls:analyzer::DetailedBeatInfo` (via `sym:CAnalyzerIF::notify_detailed_beat_info(void*, DetailedBeatInfo*, int, unsigned char)`)
- [ ] Beat unit (beats per bar) reported separately. `sym:CAnalyzerIF::notify_beat_unit(void*, int)`
- [ ] Beat list can be re-read from the database instead of re-analysed (for key / phrase only runs). `sym:CBeatAnalyzeThread::getBeatListFromDatabase`
- [ ] Beat grid editing (Grid edit panel), each an undoable action: tap tempo, apply new BPM, double / halve BPM, expand / shrink beat interval, move grid left / right / centre, move a single beat, "adjust beat from" (partial grid from here on), "adjust all beats", set 1.1 bars at playhead, set first beat, metronome on / off and volume, analysis lock toggle, undo / redo. `cls:djplay::BeatGridAdjustment` and its nested ApplyNewBpm, DoubleBeat, ExpandBeatInterval, MoveBeatPos, Set11Bars, SetFirstBeat; `str:Shift Beatgrid right`, `str:Adjust partial Beatgrid`
- [ ] BPM limits for manual edit: 40 to 499. `str:Invalid operation. BPM should be between 40 to 499.`
- [ ] Dynamic-grid warnings in the editor, "twice BPM" and out-of-range messages. `cls:djplay::BeatGridAdjustment` (ctMessageDynamic, ctMessageDynamic2, ctMessageTwiceBpm, ctMessageOutOfRange, isBpmNotOutOfRange)
- [ ] Phrase editing in the same panel: select, split, remove, clear, change phrase id. `cls:djplay::BeatGridAdjustment` (onSongStructPhraseSplit, onSongStructPhraseRemoved, onSongStructPhraseIdChanged, onSongStructPhraseCleared, popupPhraseName, initializePhraseEditButton)
- [ ] Beat grid model used by playback (section BPM lookup, quantise, ms to PPQ). `str:FormattedBeatGrid`, `str:getSectionBpm`, `str:relativelyQuantizedPosMs`, `str:positionMsToPpq`
- [ ] Real-time BPM detection of external input (not file analysis, listed for completeness; owned by mixer). `cls:rbxfrm::AutoBPMDetectorUnit`, `cls:rbxfrm::AutoBPMCalculator` (member named `bock`, **[inferred]** a Böck-style method), `cls:rbxfrm::TapBPMDetector`

### 2.3 Key

- [ ] Key analysis job, using the beat list and BPM. `sym:analyzer::AnalyzeJobKey::getKey(juce::Array<DetailedBeatInfo>&, float, shared_ptr<float>, unsigned int, bool, unsigned long, unsigned long)`
- [ ] DSP key detector inside the beat libraries: pitch detection (FFT, magnitude, Hann, low pass), candidate scoring against major / minor / third / fifth template tables, multi-step judging, "harmonic mixing" table. `cls:BeatAnalyzer_2_0` (BA_PitchDetect, BA_KeyJudge, KeyJudge_step_1, KeyJudge_step_2toFinal, Key_judge_with_keys, table_1st_maj, table_1st_min, table_3rd, table_5th, harmonic_mixing_table, final_key)
- [ ] 24 keys with display in Classic or Alphanumeric (1A to 12B) notation. `cls:analyzer` (utfKey1A ... utfKey12B, utfKeyArray, balibKeyIndex, dbKeyIDArray), `str:Key display format`, `str:Classic`, `str:Alphanumeric`
- [ ] Candidate key list (not only the winner) kept for the track. `sym:CAnalyzerIF::GetCandidateKeyindex`
- [ ] Segmented key (key per section) loader exists. `cls:analyzer` (MstLoadSegmentedKey) **[verify]** whether it is written by analysis or only read from cloud data.

### 2.4 Waveforms

- [ ] Legacy engines produce three mono waveforms: "Dots" (tiny preview), "Loud" (400-point preview) and "Zoom" (detail). `sym:BALibWrapper::GetDotsWave`, `GetLoudWave`, `GetZoomWave`, `cls:BeatAnalyzer_2_0::LoudViewMaker` (analyzeDotsColor, analyzeLoudColor, PeakDetect900), `::ZoomViewMaker`
- [ ] 100-point and 400-point previews, whole-track waveform, zoom waveform notifications. `sym:CAnalyzerIF::notify_100pt_wave`, `notify_400pt_wave`, `notify_whole_wave`, `notify_zoom_wave`
- [ ] Waveform creator from decoded float PCM: whole (preview) and detail ("zoom") colour waveforms, 3-band whole and zoom waveforms, per-track normalisation coefficient, colour calculation from three band levels. `cls:analyzer::WaveCreator` (createWave(bool,bool,bool,bool), createWholeWaveData, createColorWaveData, create3BandsWholeWaveData, create3BandsZoomWaveData, getWaveNormalizeCoef, calcZoomWaveColour(float,float,float))
- [ ] 3-band split used by the 3-band waveform. `cls:ThreeBandSeparate` (doSeparate, BAND_INDEX), `cls:analyzer::BandPassFilter`
- [ ] Colour schemes for CDJ zoom waveform. `sym:CAnalyzerIF::get_cdjZoomWaveColor`, `cls:analyzer` (hcTypeCold1, hcTypeCold2, hcTypeColorFull)
- [ ] Loaders for each waveform kind, used by the deck UI. `sym:CAnalyzerIF::load_wholeWave`, `load_blueZoomWave`, `load_colorZoomWave`, `load_3bandsWholeWave`, `load_3bandsZoomWave`
- [ ] Waveform images for hardware jog displays and video. `sym:CAnalyzerIF::write_image_on_jog_display`, `writeVideoImage`, `create_rescaled_image`

### 2.5 Phrase / song structure

- [ ] Rule-based (DSP, not ML) phrase analyser working on the beat grid: band power and peak analysis, kick / snare / hi-hat detection, fills, breaks, intro end, outro, verse / hook (chorus) judgement, "up" / build sections, bar start correction. `cls:analyzer::SongStructureAnalyze` (LowPowerAnalyze16, HiHatAnalyze_a, DetectSnarePositions, FillinSnareAna, BreakFind, IntroEndJudge, OutroKickoutAna, VerseHookJudge_All, UpFeatureAna, BarStartCorrection)
- [ ] Genre-type judgement that selects the phrase vocabulary (mood): four-on-the-floor pop / rock, 128 BPM EDM style, slow tempo, slow rock, fade out. `cls:analyzer::SongStructureAnalyze` (SongStructureAna_Type_Genre_Judge, Genre12FinalJudge, Detect4KicksPopRockRhythm, DetectBPM128, DetectSlowTempo, DetectSlowrock, DetectFadeOut, DetectTenkeiGenre1)
- [ ] Vocal entry detection inside phrase analysis. `cls:analyzer::SongStructureAnalyze` (VocalInDetection, UtaUmuAnalyze_a, **[inferred]** 歌有無 = vocal present or not)
- [ ] Result model: phrase list with ids, kinds and positions; stored per track. `cls:analyzer::SongStructureAnalyzeResult`, `cls:analyzer::SongStructInfo`, `cls:analyzer::PhraseInfo`, `sym:CMusicAnalyzeFile::set_pmai_ext_song_struct_data`
- [ ] Phrase display in waveforms (full and enlarged) and phrase names. `str:Phrase (Enlarged Waveform)`, `str:Phrase (Full Waveform)`, `str:showPhraseNameAlways`
- [ ] Song structure scrambled when exported to USB (as on CDJ-3000 media). `cls:analyzer` (ConvUSB_ScrambleSongStruct)
- [ ] Phrase data consumed by lighting (scene per phrase) and by cue prediction / personal trend. `sym:djplay::CDJLightingPlayer::getPhraseIdByPhrase(analyzer::PhraseInfo*, unsigned short)`, `sym:analyzer::PersonalTrendAnalyzer::analyzeTrack(..., SongStructureAnalyzeResult const&)`

### 2.6 Vocal detection

- [ ] Per-frame vocal / non-vocal classification with a 2-layer bidirectional LSTM (TensorFlow frozen graph `vd_param`), loaded through the TensorFlow C API at runtime. `cls:analyzer::VocalDetector` (setData(float*, int, int), applyEndProcess, getResult), `sym:analyzer::Discriminator::inference(double*, int, int&)`, `sym:tf_utils::LoadGraph(juce::DynamicLibrary&, juce::String, char const*, TSL_Status*)`, `file:Resources/vd_param.pb`
- [ ] Hand-written "librosa-like" feature front end: STFT, mel spectrogram, power to dB, MFCC and MFCC deltas, fluctogram (Dolph-Chebyshev window), feature normalisation. `cls:analyzer::Librosa` (get_melspec, get_mfcc, get_mfcc_delta, calc_fluctogram, chebwin, stft_interp, normalize_feature)
- [ ] Post-processing ("end process" smoothing) to a byte-per-point vocal array with a version number, stored in `.2EX`. `sym:CMusicAnalyzeFile::set_pmai_ex2_vocal_detect_data`, `set_pmai_ex2_vocal_detect_version(int, unsigned short)`
- [ ] Vocal position display (full and enlarged waveform). `str:Vocal (Full Waveform)`, `str:Vocal (Enlarged Waveform)`, `str:Display the vocal position in the song.`, `cls:djplay::VocalDetectView`
- [ ] Vocal array scrambled on USB export. `cls:analyzer` (ConvUSB_ScrambleVocalDetect)
- [ ] Vocal percentage used by Track Suggestion filters ("including vocals" threshold). `sym:feature_extractor::get_percentage_vocals`, `get_annotated_vocals`, `str:including_vocals_threshold`

### 2.7 Loudness and Auto Gain

- [ ] Integrated loudness and sample peak over the whole track (stereo float input). `cls:analyzer::LoudnessAnalyzer` (init(unsigned int, unsigned int), addBuffer(float const*, float const*, ...), calcLoudness, calcSamplePeak)
- [ ] Auto Gain dB derived as reference level minus measured loudness. `cls:analyzer::Loudness2AutoGainConverter` (mStandardLoudnessLevel, trackLoudness2AutoGaindB(double))
- [ ] Auto Gain applied on the deck, with a knob that adjusts it and Trim / Auto Gain switch. `str:Auto Gain knob:`, `str:Trim knob/Auto Gain Switch button:`, `sym:djengine::DjEngineIF::getAutoGain`, `getAutoGainType`
- [ ] Gain and peak are part of cloud analysis records. `str:track_gain`, `str:track_peak`

### 2.8 Intelligent Cue (cue point prediction)

- [ ] Preferences > Analysis > CUE Analysis: "Set CUE during analysis" with Auto or Manual, CUE type, max hot cues, max memory cues, "Set to 1.1Bars", repeat CUE per N bars, enable cue detect, disallow overwriting CUE, add comments to CUE. `str:Intelligent Cue Creation`, `str:CUE Type`, `str:maxHotCue`, `str:maxMemoryCue`, `str:Set to 1.1Bars`, `str:Repeat CUE settings per setting value of bars`, `str:Disallow overwriting of CUE`, `str:Add comments to CUE`, `cls:AnalysisAutoCue`
- [ ] "CUE Analysis Playlist": tracks with user-set cues are analysed to learn the user's personal cue tendency. `str:CUE Analysis Playlist`, `str:analyzes your CUE points information (personal CUE Tendency Information)`
- [ ] Personal trend model: per phrase-structure matrix of how often the user places cues at each beat offset towards the next structure boundary, min-max normalised, updated incrementally. `cls:analyzer::PersonalTrendAnalyzer` (structure_value_matrix, vector_of_active_beat_to_next_structure, updatePersonalTrendMatrix, getPersonalTrendPositionDistribution), `sym:db::DatabaseIF::StartPersonalTrend`, `sym:CAnalyzerIF::AnalyzePersonalTrend`
- [ ] ML scoring of candidate cue positions: SampleCNN audio features feed a gradient boosted tree regressor (both ONNX). `cls:analyzer::CuePointPredictor` (initilaize_session, onnx_get_features, setData(float*, int), getScores(int, vector<long long> const&, SongStructureAnalyzeResult const&)), `file:models/detect_cue_model/*.onnx`
- [ ] Final score = model score combined with personal trend score, adjusted near beats, normalised, sorted; top N become hot / memory cues. `cls:analyzer::ScoreCalculator` (getTotalScores, getPersonalTrendScoreMap, adjustScoreNearBeat, minMaxNormalization), `cls:analyzer::DetectCue`
- [ ] Predictor session is shared and torn down at shutdown. `cls:analyzer` (finalizeCuePointPredictor)

### 2.9 Embeddings, energy, genre and related tracks

- [ ] Load audio for embedding with min / max usable duration. `sym:feature_extractor::load_audio(string const&, int, bool)`, `cls:feature_extractor` (AVAILABLE_MIN_SEC, AVAILABLE_MAX_SEC)
- [ ] Segment audio into overlapping windows and run a spectrogram CNN to get a 64-d feature (plus 50 tag probabilities) per window. `sym:feature_extractor::unfold<float>`, `sym:feature_extractor::FeatureExtractor::cnn_run`, `get_features(nc::NdArray<float>, int, int)`, `file:models/embed_model/cnn_embedding.onnx`
- [ ] Average window features into a track vector; normalise; project to 2-D with a parametric UMAP network; normalise again. `sym:feature_extractor::Embedder::avg_feats`, `get_umap_2d_from_feats`, `file:norm_before_umap.onnx`, `umap_embedder.onnx`, `norm_after_umap.onnx`
- [ ] Energy score (graph is named "model_danceability") from the 64-d feature. `sym:Embedder::get_energy_from_feats`, `file:model_energy.onnx`
- [ ] Genre classifier over 63 fine-grained genres, with "detailed" and "coarse" modes and a genre synonym list. `sym:Embedder::get_genre_from_feats(feats, feats)`, `file:model_genre.onnx`, `str:genre_mode`, `str:detailed`, `str:coarse`, `str:bass house|melodic dubstep|j-pop,jpop|...`
- [ ] Stored per track as MessagePack JSON with keys `bpm, emb_2d, emb_hd, emb_hds, energy, genre, hash, path, track_id, track_keycode, track_length, vocals`. `cls:feature_extractor::key`, `sym:feature_extractor::get_messagepack_from_json`, `sym:CAnalyzerIF::setEmbeddingData`, `getEmbeddingData`
- [ ] Nearest neighbour search over embeddings with BPM / key / genre / vocal filters and exponentially weighted random sampling (Track Suggestion, Collection Radar). `cls:feature_extractor::Handler` (get_neighbors, filter_choices_by_bpm_key_genre_vocals, exponential_random_sample, get_filter_genres), `cls:db::TrackSuggestion`
- [ ] Cloud recommendation: register embeddings, query status and neighbours on the server (Streaming Radar). `str:/client/v%d/recommendation/embedding/register`, `str:/client/v%d/recommendation/tracks/neighbors`, `cls:AIRecommendTrackItemEmbedding`, `sym:CAnalyzerIF::GetNetworkRecommendData`, `RequestUploadRecommendTrack`

### 2.10 Cloud analysis

- [ ] Search the cloud for pre-computed analysis of the same track, download ANLZ blocks and write them locally, greatly reducing analysis time. `cls:analyzer::AnalyzeJobCloudSearch`, `cls:analyzer::AnalyzeJobCloudDownload`, `str:https://rb-share.kuvo.com/anlz/v1/track/search`, `str:It retrieves analysis information from the Internet and greatly reduces analysis time.`, `sym:CAnalyzerIF::WriteDonwloadFile`
- [ ] Upload own analysis (opt-in "Participate in rekordbox data collection"). Record fields include title, artist, album, key tag, genre, ISRC, BPM, key, gain, peak, phrase, analysis hash, audio quality, auto flag. `str:https://rb-share.kuvo.com/anlz/v1/track/upload`, `str:Participate in rekordbox data collection`, `str:track_isrc|track_bpm|track_key|track_gain|track_peak|data_url|...|analysis_hash`
- [ ] Server configuration fetch. `str:https://rb-share.kuvo.com/anlz/v1/configuration`
- [ ] Cloud Analysis toggle in preferences and for DJ equipment (Cloud Direct Play). `cls:AnalysisTrackAnalysisCloudAnalysisComponent`, `str:Use Cloud Analysis with DJ equipment`

Cuepoint has no access to this service. Parity target: the UI toggle exists and is off; the job slots exist so a future Cuepoint-hosted service can plug in.

### 2.11 Stem separation (Track Separation / STEMS)

- [ ] Two separation engines loaded as plug-in dylibs: Spleeter (TensorFlow) and Demucs v3 (ONNX Runtime). `file:Frameworks/libSpleeter.dylib`, `file:Frameworks/libDemucs3.dylib`, `dylib:_demucs3Initialize`, `dylib:_spleeterInitialize`, `str:libSpleeter.dylib`, `str:libDemucs3.dylib`
- [ ] Two stem layouts: 3 stems (drums, vocal, inst = other with bass) and 4 stems (drums, vocal, inst, bass). `dylib:_demucs3OperationAtIndexOtherWithBass`, `_demucs3OperationAtIndexSeparatedBass`, `str:DRUMS-VOCAL-INST`, `str:DRUMS-VOCAL-INST-BASS`
- [ ] Engine API: Initialize / Finalize / IsInitialized / PreOperation / PostOperation / CancelOperation / OperationAtIndex (chunked, random access) / OperationAtOnce (whole buffer). Buffers are planar stereo: 2 input planes and up to 4 stereo stem outputs (10 float pointers). `dylib:demucs3::OperationAtOnce(unsigned int, float*, ... x10)`
- [ ] Model weights are not bundled; they are downloaded on demand as versioned zips from `dl.rekordbox.com/application/models/{stems,demucs3}/`, uncompressed into a models directory, with update prompts. `str:https://dl.rekordbox.com/application/models/demucs3/`, `str:demucs3_ver.txt`, `str:demucs3_xxxx.zip`, `str:STEMS Engine is updated. Do you want to download it?`, `cls:rbxfrm::audiopart::AudioPartDownloader`, `AudioPartUncompressor`
- [ ] Separation runs per deck, asynchronously, writing stem audio to a cache; playback can start once the "ready position" passes the play head. Memory limits, parallel analysis and large-memory mode options. `cls:rbxfrm::audiopart::AudioPartSeparatorManager` (asyncRequestSeparate, registerDeck, unregisterDeck), `cls:rbxfrm::audiopart::AudioPartData` (getReadyPosition, isReadyPosition), `cls:rbxfrm::audiopart::AudioPart` (getAvailableMemorySec, setAudioPartParallelAnalyze, setAudioPartUsageLargeMemory)
- [ ] Stem URIs of the form `model:part://...` addressing a separated part. `str:unknownmodel:unknownpart://`, `cls:rbxfrm::audiopart::PREFIX`

Detailed stem playback (Active Stem, Stem ISO, stem FX) belongs to the deck / mixer subsystem; this section covers only the separation engine.

### 2.12 ANLZ writing

- [ ] Three files per track: `.DAT`, `.EXT` and `.2EX` ("pmai", "pmai_ext", "pmai_ex2" in the code), in `PIONEER/USBANLZ/Pnnn/xxxxxxxx/ANLZ0000.*` style folders, with header, path, cue, beat grid, waveform, VBR, seek table, song structure, 3-band waveform, normalise coefficient and vocal tags. `cls:analyzer::CMusicAnalyzeFile`, `str:ANLZ0000.DAT`, `str:ANLZ0000.EXT`, `str:/ANLZ%04X.DAT`, `str:PIONEER/USBANLZ`
- [ ] Unknown atoms are preserved on read. `sym:CMusicAnalyzeFile::read_pmai_ext_unknown_atom`
- [ ] Per-file update flags and file locking. `cls:analyzer::MusicAnalyzeFileScopedLock`, `sym:CMusicAnalyzeFile::set_pmai_ext_update_flag`

### Parity checklist (summary)

| Area | Must match rekordbox | Notes |
|---|---|---|
| BPM (constant) | within 0.01 BPM on 95% of corpus, octave correct for chosen range | |
| Beat grid offset | median |offset| below 5 ms | first-beat choice matters for CDJ sync |
| Downbeat / bar 1 | same downbeat on 95% | |
| Dynamic grid | per-beat times within 10 ms | Dynamic / Auto mode |
| Key | exact on 80%, MIREX weighted score at least 0.9 | |
| Waveforms | byte-level layout identical; values within tolerance | hardware reads them |
| Phrases | boundary F-measure at 1 bar at least 0.85; label agreement at least 0.8; same mood | |
| Vocal | frame agreement at least 90% | |
| Loudness / Auto Gain | within 0.2 dB | |
| Intelligent Cue | at least 70% of cues within 1 beat | personalisation separately |
| Embeddings | neighbour overlap and genre agreement | our own model, so "behavioural parity" only |
| Stems | SDR comparable to open Demucs | weights differ |

## 3. Models and data artefacts

All paths are relative to `rekordbox.app/Contents/`. Shapes come from parsing the files offline (ONNX via onnxruntime metadata and a hand protobuf walk; TF SavedModel via keras_metadata.pb and the TensorBundle index; vd_param via its GraphDef). None of these files may be copied into Cuepoint.

| Path | Size (bytes) | Framework / format | Inputs to outputs | Inferred purpose | Cuepoint replacement |
|---|---|---|---|---|---|
| `Resources/models/detect_beat/model7/` (saved_model.pb 7,422,095; variables.data 423,654; variables.index 2,246; keras_metadata.pb 3,996) | ~7.85 MB | TensorFlow 2 SavedModel, Keras 2.12 custom model `TestLayerModel`, serving signature `serving_default`, input `fltp` | `fltp` float32 [batch, samples] mono PCM; in-graph STFT at frame lengths 1024 / 2048 / 4096, hop 441 (100 frames/s at 44.1 kHz), "FilteredSpectrogram" (log filterbank) plus positive spectral difference to 314 features per frame; 3 stacked bidirectional peephole-LSTM layers, 25 units per direction; dense 50 to 3 with softmax. 99,004 parameters. | High precision beat and downbeat activation (classes: beat, downbeat, no beat). Architecture is identical to madmom's `RNNDownBeatProcessor` BLSTM (Böck, Krebs, Widmer 2016) **[inferred]**; post-processing (DBN / HMM decoding to beats and bars) is presumably in C++ **[verify]**. | Train our own BLSTM of the same shape, or use Beat This! (MIT code and weights). Do not use madmom's shipped weights (CC BY-NC-SA 4.0, non-commercial). |
| `Resources/models/detect_beat/model8/` | ~7.85 MB | Same as model7 | Same shapes, different weights | Second member of an ensemble, or a variant (for example dynamic vs constant tempo) **[inferred]**. madmom ships eight downbeat BLSTMs named `_1` to `_8`; names 7 and 8 are suggestive **[inferred]**. Section 6 asks how both are used. | As above; ensemble of 2 trained seeds if parity needs it. |
| `Resources/vd_param_raw.pb` | 6,758,758 | TensorFlow 1.x frozen `GraphDef` (plain protobuf) | `InputBatch` float [batch, 29, time] (**[inferred]** 29 features per frame) and `LengthBatch`; 2-layer bidirectional `BasicLSTMCell`, 256 units per direction (kernels [285,1024] and [512,1024]); FC 512 to 64 ReLU, 64 to 2, softmax. About 1.67 M parameters. | Vocal detection, per frame vocal / non-vocal probability. "vd" = vocal detection. Feature front end is `analyzer::Librosa` (MFCC, deltas, fluctogram), consistent with Lehner et al. style singing voice detection **[inferred]**. | Train a small BiLSTM or CRNN on open vocal-activity data (Jamendo Corpus, MedleyDB, MUSDB18 vocal stems energy), or derive vocal activity from our own stem separator's vocal stem energy. |
| `Resources/vd_param.pb` | 6,758,760 | Same graph, encrypted / obfuscated (byte entropy 8.0 bits, 2 bytes longer) | n/a | The copy the app actually loads; `tf_utils::LoadGraph` takes an extra `char const*` (a key) **[inferred]**. The raw file is shipped alongside, apparently by mistake. | Not needed. We never record or use the key. |
| `Resources/models/detect_cue_model/samplecnn.onnx` | 7,459,554 | ONNX (PyTorch export, opset 12) | `waveform` [batch, 59049] (3^10 samples, about 2.7 s at 22.05 kHz or 3.7 s at 16 kHz **[verify]**) to `tags` [batch, 50] sigmoid and `features` [batch, 512]. 11 Conv1d (kernel 3) + MaxPool(3) blocks, 128 to 256 to 512 channels; 1.86 M parameters. | SampleCNN (Lee et al. 2017) music auto-tagger, 50 tags (MagnaTagATune top-50 style) **[inferred]**; used as an audio feature extractor around cue candidates. | Train SampleCNN ourselves on MTG-Jamendo (CC licensed audio) using the MIT `sota-music-tagging-models` code, or reuse the same network that drives our embeddings. |
| `Resources/models/detect_cue_model/cue_prediction_model_no_abs_position.onnx` | 547,505 | ONNX-ML (OnnxMLTools), single `TreeEnsembleRegressor` | `input` double [N, F], F up to 3,085 (2,378 distinct feature ids used) to `variable` [N, 1]. 850 trees, 16,782 nodes, `BRANCH_LT`, base value 0.5. | Cue-worthiness score per candidate beat. Base 0.5 and LT branching point to XGBoost **[inferred]**. "no_abs_position" means absolute time in the track is deliberately not a feature. F is close to 6 x 512 SampleCNN features + 13 context features **[inferred]**. | Train a gradient boosted model (Rust `gbdt`-style or LightGBM offline, exported to our own JSON tree format) on cues from our own users who opt in, or from rekordbox-exported libraries we are allowed to use; evaluate in pure Rust (trivial tree walk). |
| `Resources/models/embed_model/cnn_embedding.onnx` | 1,894,485 | ONNX (opset 17, merged via `onnx.compose`) | `signal` [channels, samples], `frame_step`, `frame_length`, `pads`; in-graph Hann STFT (257 bins, so n_fft 512), 96 mel bands, log, batch norm; 5 Conv2d 3x3 (64,128,128,128,64) with max pools (2,4),(2,4),(2,4),(3,5),(4,4); dense 64 to 50 sigmoid. Outputs `output` [n, 50] and `features` [n, 64]. 471 k parameters. | FCN music tagger (Choi et al. 2016) as in `sota-music-tagging-models` **[inferred]**; 64-d penultimate layer is the track embedding. | Train FCN (or a newer small tagger) on MTG-Jamendo with MIT code; export to ONNX or safetensors for tract / candle. |
| `Resources/models/embed_model/norm_before_umap.onnx` | 913 | ONNX-ML `Scaler` (skl2onnx) | [N, 64] to [N, 64] | Standardisation before UMAP. | Our own scaler parameters (JSON). |
| `Resources/models/embed_model/umap_embedder.onnx` | 5,409,105 | ONNX (tf2onnx) MLP | `input_1` [N, 64] to `dense_6` [N, 2]; 7 dense layers 512 wide with PReLU; 1.35 M parameters | Parametric UMAP projection to the 2-D "map" used by Radar views. | Fit parametric UMAP (BSD-3 `umap-learn`) on our embeddings, export MLP weights. |
| `Resources/models/embed_model/norm_after_umap.onnx` | 290 | ONNX-ML `Scaler` | [N, 2] to [N, 2] | Rescale 2-D map coordinates. | JSON constants. |
| `Resources/models/embed_model/model_energy.onnx` | 1,874,757 | ONNX-ML `Scaler` + `TreeEnsembleRegressor` (graph named `model_danceability`) | `input_features` [N, 64] to [N, 1]; 889 trees, 49,529 nodes, base 0.698 | Energy (labelled danceability in training) score for a track. | Train a regressor on open danceability / energy labels (for example AcousticBrainz high-level data, CC0) against our embedding. |
| `Resources/models/embed_model/model_genre.onnx` | 52,499 | ONNX-ML MLP (skl2onnx MLPClassifier) | `input_features` [N, 66] (**[inferred]** 64-d embedding + 2-d UMAP) to label int64 and probabilities over 63 classes; layers 66-64-64-63 | Fine-grained genre (63 classes, list in strings, many with synonyms) with "coarse" grouping. | Train on MTG-Jamendo genre tags or Discogs-derived labels with our own class list; match the coarse groups. |
| `Frameworks/libDemucs3.dylib` | 802,816 | Native code over ONNX Runtime (`OrtGetApiBase`); weights downloaded separately (`demucs3_xxxx.zip`) | Planar stereo in, 3 or 4 stereo stems out | Demucs v3 (hybrid) stem separation | Open Demucs (MIT code; pretrained weights published by Meta in the same repo **[verify]** licence of each checkpoint), run with candle or burn. |
| `Frameworks/libSpleeter.dylib` | 451,840 | Native code over TensorFlow C++ (`TFWrapper::Model` built from an in-memory buffer, in-graph STFT via TF ops) | As above | Spleeter (older engine, kept for compatibility) | Not needed if Demucs path ships; Spleeter code and 2/4/5-stem weights are MIT if ever wanted. |
| `Frameworks/libonnxruntime.1.18.0.dylib` | 71 MB | ONNX Runtime 1.18, CoreML EP imported | n/a | Runtime for all ONNX models and Demucs | tract / candle (pure Rust). |
| `Frameworks/libtensorflow_cc.2.16.1.dylib`, `libtensorflow_framework.2.16.1.dylib` | 1.2 GB and 78 MB | TensorFlow 2.16.1 | n/a | Runtime for detect_beat, vd_param, Spleeter | Not needed. |

Note on downloaded stem models: the zips (`separator_*.zip`, `demucs3_*.zip`) are not in the bundle and were not inspected.

## 4. Analysis pipeline as observed

### 4.1 Flow

1. **Request.** UI or database layer (`db::DatabaseIF::analyzeCollectionStart`, auto analysis on import / load / export) calls `CAnalyzerIF::start_analyze` with a data source (local path, streaming URI or cloud file) and a `BeatAnalyzeReq` built from `AnalyzeParametorManager` (mode, BPM range, cue mode, enabled stages) and `AnalyzeJobManager` flags.
2. **Thread.** `CBeatAnalyzeThread::startAnalyze` spawns a worker (`run`). `analyze()` drives an `AnalyzeJobController` whose jobs share a `ThreadOwner`, a callback, both managers, an `AnalyzerContentManager` and `CommonSettings`. Each job implements `Initialize`, `Job`, `Detect`, `GetResult`, `SetSpecialInfo`.
3. **Cloud first [inferred order].** `AnalyzeJobCloudSearch` then `AnalyzeJobCloudDownload`; on a hit the downloaded ANLZ blocks are written and the remaining DSP stages can be skipped.
4. **Decode.** PCM is read through `rbxfrm::PcmReaderTemplate<Float2>` (`CBeatAnalyzeThread::getStreamAt` produces both 16-bit and float buffers). FLAC builds a seek table; MP3 builds a VBR frame index.
5. **Beat.** `AnalyzeJobBeatDetector`: either `legacyBeatAnalyze` (BALib 1.0 or 2.0, Const or Varied constructor at 44.1 or 48 kHz) or `highPrecisionBeatAnalyze` (TF model). Then BPM range fit (double / half), first downbeat, renumbering, conversion to microseconds. Legacy engine also emits Dots / Loud / Zoom waveforms during the same pass.
6. **Key.** `AnalyzeJobKey::getKey` with the beat list, BPM and the float PCM.
7. **Waveforms.** `WaveCreator::createWave(bool,bool,bool,bool)` produces colour preview and detail, 3-band preview and detail, and the normalisation coefficient (four flags select which **[inferred]**).
8. **Phrase.** `SongStructureAnalyze::Initialize(6 ints, 3 bools, result)` then analysis over the beat grid.
9. **Vocal.** `Librosa` features, then `Discriminator` (TF graph), then `VocalDetector::applyEndProcess`.
10. **Loudness.** `LoudnessAnalyzer` over the whole file, `Loudness2AutoGainConverter` to dB.
11. **Embedding.** `feature_extractor::embed(path, int, bool, bool)` with the shared ONNX sessions (`Embedder`, `FeatureExtractor` keep weak / shared pointers so sessions are reused across tracks).
12. **Cue.** If CUE analysis is on: `CuePointPredictor` scores candidates, `ScoreCalculator` mixes in `PersonalTrendAnalyzer`, `DetectCue` writes cues (respecting "disallow overwriting").
13. **Notify.** Each stage calls back through `CBeatAnalyzeThread::notify*` into `CAnalyzerIF::notify_*`, which writes ANLZ tags and database fields (BPM, key, analysed flags, AnalysisDataPath, AnalysisUpdated). `notify_complete` ends the job; `notify_progress` drives the progress bar.

Engine-level notes:

- Stem separation is a separate pipeline (`rbxfrm::audiopart`), triggered by loading a track to a deck with STEMS on, not by Track Analysis.
- High-precision mode is documented in the UI as +90% (Normal) and +250% (Dynamic) time **[inferred mapping]**.
- TF is loaded through `juce::DynamicLibrary` at runtime (`tf_utils::*` takes the library handle), so analysis still works if TF fails to load **[inferred]**.

### 4.2 ANLZ tags produced

Mapping from `CMusicAnalyzeFile` method names to the public tag names documented by Deep Symmetry is **[inferred]** from name and file:

| File | Method family | Tag | Content |
|---|---|---|---|
| DAT | `*_pmai_path` | PPTH | audio path |
| DAT | `*_pmai_vbr` | PVBR | MP3 VBR index |
| DAT | `*_pmai_quantize`, `set_pmai_beat_position` | PQTZ | beat grid (beat number, tempo x100, time ms) |
| DAT | `*_pmai_wave400pt` | PWAV | 400-column mono preview |
| DAT | `*_pmai_wave100pt` | PWV2 | 100-column tiny preview |
| DAT | `*_pmai_hot_cue`, `*_memory_cue` | PCOB | legacy cue lists |
| EXT | `*_pmai_ext_path` | PPTH | |
| EXT | `*_pmai_ext_zoom_wave` | PWV3 | mono detail (150 per second) |
| EXT | `*_pmai_ext_new_whole_wave` | PWV4 | colour preview |
| EXT | `*_pmai_ext_new_zoom_wave` | PWV5 | colour detail |
| EXT | `*_pmai_ext_cue*`, `cue_ext` | PCOB / PCO2 | cues with colour, comment, loop length |
| EXT | `*_pmai_ext_quantize_ext`, `ext_beat_position` | PQT2 | extended (high precision / dynamic) grid |
| EXT | `*_pmai_ext_song_struct` | PSSI | phrases (scrambled on USB export) |
| EXT | `*_pmai_ext_seek_table` | unknown | seek table **[verify tag]** |
| 2EX | `*_pmai_ex2_3bands_whole_wave` | PWV6 | 3-band preview |
| 2EX | `*_pmai_ex2_3bands_zoom_wave` | PWV7 | 3-band detail |
| 2EX | `*_pmai_ex2_wave_normalize_coef` | PWVC | waveform normalisation |
| 2EX | `*_pmai_ex2_vocal_detect` | unknown | vocal array + version **[verify tag]** |

Embedding data is not in ANLZ; it lives in the database (`setEmbeddingData`, MessagePack JSON). Auto Gain and peak live in the database **[verify]**.

### 4.3 Settings that affect analysis

Analysis Mode (Normal / Dynamic / Auto), BPM Range, high precision on / off, enabled stages (BPM/Grid, Key, Phrase, Vocal, Embedding), Cloud Analysis on / off and upload opt-in, CUE analysis (on, Auto / Manual, type, counts, 1.1 bars, repeat per bars, overwrite, comments), Key display format (display only), Analysis Lock (per track), STEMS model choice and memory options (separation only).

## 5. Prior art and licences

| Item | Use for us | Licence | Notes |
|---|---|---|---|
| Deep Symmetry, "rekordbox ANLZ files" analysis (`djl-analysis` docs, crate-digger) | ANLZ tag layouts, PSSI phrase kinds and mood, scrambling | Docs: CC BY 4.0 **[verify]**; crate-digger code EPL 2.0 | Read the docs; do not copy EPL code. |
| madmom (Böck et al.) | Algorithms: multi-resolution log filtered spectrogram, BLSTM beat / downbeat activations, DBN bar tracker | Code BSD 2-clause; model files CC BY-NC-SA 4.0 (commercial use needs permission) | Reimplement the algorithm from the papers; never ship madmom weights. |
| Beat This! (Foscarin, Schlüter, Widmer, ISMIR 2024) | Open beat / downbeat model, no DBN needed | MIT (code and published weights) | Best candidate for high precision mode. |
| BeatNet | Online beat / downbeat / meter | **[verify]** (believed CC BY 4.0) | Optional. |
| Essentia | Reference for key (HPCP + profiles), loudness, tempo | AGPL 3.0; most Essentia models CC BY-NC-SA | Reference only, not linked. |
| libKeyFinder | Key detection reference | GPL 3.0 | Reference only. |
| Key profiles (Krumhansl-Kessler, Temperley, EDMA / Faraldo) | Template-matching key detector | Published research | Implement from papers. |
| EBU R128 / ITU-R BS.1770-4 | Loudness | Open standards | `ebur128` Rust crate, MIT. |
| sota-music-tagging-models (Won et al.) | FCN, SampleCNN, short-chunk CNN training code | MIT (code); weights not clearly licensed | Train our own weights. |
| MTG-Jamendo dataset | Training audio and tags | Audio CC licences per track (mix of BY, BY-SA, BY-NC*) | Filter to commercial-friendly tracks for shipped weights. |
| umap-learn (parametric UMAP) | 2-D map | BSD 3-clause | |
| Demucs (Défossez, Meta) | Stem separation, v3 hybrid and v4 htdemucs | MIT code; pretrained weights in repo **[verify per checkpoint]** | Repo archived, still usable. |
| Spleeter (Deezer) | Stem separation | MIT including pretrained models | Fallback. |
| Singing voice detection (Lehner, Widmer, Böck 2015; Schlüter 2016) | Vocal detection features (fluctogram, MFCC) and CNN / RNN designs | Papers | Implement from papers. |
| candle (Hugging Face) | Pure-Rust tensor and NN runtime | MIT / Apache 2.0 | Good for LSTM, Demucs. |
| burn | Pure-Rust NN framework with ONNX import | MIT / Apache 2.0 | Alternative to candle. |
| tract (Sonos) | Pure-Rust ONNX / NNEF inference | MIT / Apache 2.0 | Good for small CNNs; check STFT op support **[verify]**. |
| rustfft, realfft, rubato, symphonia | FFT, resampling, decoding | MIT / Apache 2.0 (symphonia MPL 2.0) | |

## 6. Open questions for Ghidra

Each item names the symbol to decompile and what it answers.

1. `analyzer::AnalyzeJobBeatDetector::highPrecisionBeatAnalyze()`: which of model7 / model8 is used when (mode, sample rate, ensemble average), how the 3-class activations are decoded to beats and downbeats (DBN, Viterbi, peak picking), and how a dynamic grid is formed.
2. `TFWrapper::Model::Model(string const&, bool, int, bool)` and `TFWrapper::Model::run` in the main binary: input resampling (does it feed 44.1 kHz mono?), batch / chunk size, any normalisation of `fltp`.
3. `analyzer::AnalyzeJobBeatDetector::legacyBeatAnalyze()` and `analyzer::BALibWrapper::Create(bool, unsigned long, int, int, bool, bool, bool)`: meaning of each parameter, when 1.0 vs 2.0 is chosen (`BALibVersion`), what "Auto" mode decides on.
4. `analyzer::AnalyzeJobBeatDetector::bpmToDouble` / `bpmToHalf` / `getFirstBeatPos()::convFirstBar` / `renumberBeat`: exact BPM range folding rule and first-bar rule.
5. `analyzer::AnalyzeJobManager::SetUrahaku` and its consumer: what off-beat correction does.
6. `BeatAnalyzer_2_0::UnitBeatAdjust::analyze` and `peakss::BeatInfoAnalyzer::judgeHendouDynamicPeak`: how tempo change points are found in Dynamic mode.
7. `analyzer::AnalyzeJobKey::getKey` and `BeatAnalyzer_2_0::BA_GetKey` / `KeyJudge_step_2toFinal`: frame size, pitch range, template values (we only need the method, not the tables), what "harmonic_mixing_table" changes, and how the candidate key list is ordered.
8. `analyzer::WaveCreator::createWave(bool,bool,bool,bool)`, `calcZoomWaveColour(float,float,float)`, `create3BandsZoomWaveData`, `getWaveNormalizeCoef`, and `ThreeBandSeparate::ThreeBandSeparate(int,int)`: band crossover frequencies, column rate, value scaling, colour formula; this is needed for byte-level parity.
9. `BeatAnalyzer_2_0::LoudViewMaker::analyzeLoudColor` / `analyzeDotsColor` / `ZoomViewMaker::analyzeZV`: height and whiteness scaling for PWAV, PWV2, PWV3.
10. `analyzer::SongStructureAnalyze::Initialize(int,int,int,int,int,int,bool,bool,bool,...)`, `SongStructureJudge`, `SongStructureAna_Type_Genre_Judge`: parameter meaning, mood decision rule, phrase label set per mood, and how fills and bank are set.
11. `analyzer::CMusicAnalyzeFile::set_pmai_ext_song_struct_data` and `analyzer::ConvUSB_ScrambleSongStruct` / `ConvUSB_ScrambleVocalDetect`: on-disk layout and the scrambling rule for USB (structure only; record no secret constants beyond what Deep Symmetry has published).
12. `analyzer::CMusicAnalyzeFile::init_pmai_ex2_vocal_detect`, `read_pmai_ex2_vocal_detect`, `read_pmai_ext_seek_table`: four-character tags and layouts for vocal and seek table atoms.
13. `analyzer::Librosa::Librosa(int, double)`, `get_mfcc`, `calc_fluctogram`, and `analyzer::Discriminator::inference(double*, int, int&)`: exact 29 features, sample rate, hop, normalisation; how the window is chunked into the BiLSTM.
14. `analyzer::VocalDetector::applyEndProcess` and `getResult`: smoothing / thresholds, points per second of the stored vocal array, version number.
15. `analyzer::LoudnessAnalyzer::calcLoudness` and `analyzer::Loudness2AutoGainConverter::trackLoudness2AutoGaindB` plus the value of `mStandardLoudnessLevel`: is it BS.1770 gated integrated loudness, what reference level, any clamping of gain.
16. `analyzer::CuePointPredictor::onnx_get_features` and `getScores`: sample rate and offsets of SampleCNN windows, layout of the 3,085 feature vector, candidate positions.
17. `analyzer::ScoreCalculator::getTotalScores`, `adjustScoreNearBeat`, `getPersonalTrendScoreMap`, and `analyzer::PersonalTrendAnalyzer::updatePersonalTrendMatrix`: weighting between model and personal trend, how hot vs memory cues are chosen, minimum spacing.
18. `feature_extractor::embed`, `FeatureExtractor::get_features(nc::NdArray<float>, int, int)` and `AVAILABLE_MIN_SEC` / `AVAILABLE_MAX_SEC`: sample rate, window and hop, which part of the track is used, what `emb_hds` holds.
19. `feature_extractor::Embedder::get_genre_from_feats`: confirm the 66-d input composition and the coarse grouping.
20. `feature_extractor::Handler::get_neighbors` and `exponential_random_sample`: distance metric, k, randomisation parameter, filter tolerances for BPM and key.
21. `analyzer::AnalyzeJobController::Start` and `analyzer::CBeatAnalyzeThread::analyze()`: true job order, which stages are skipped on cloud hit, number of concurrent analysis threads.
22. `analyzer::CAnalyzerIF::notify_complete` and `db::DatabaseIF` analysis writers: which database columns are written (BPM x100, KeyID, Analysed bit flags, AutoGain / peak location).
23. `rbxfrm::audiopart::AudioPartSeparator::run` and `demucs3::OperationAtIndex`: chunk length, overlap, sample rate, and output scaling for stems.

## 7. Rust plan

### 7.1 Crates and modules

- `cp-analysis`: orchestrator. Job graph mirroring section 4.1 (`Job` trait with `prepare`, `run`, `result`), a bounded worker pool (default one track per core minus one, configurable), cancellation tokens, progress channel to the egui UI, settings struct (mode, BPM range, stages, cue options). Writes through `cp-anlz` and the database crate.
- `cp-anlz`: ANLZ reader / writer (owned by the export subsystem, used here). Unknown atoms preserved.
- `cp-dsp`: STFT (realfft), filterbanks (log-frequency and mel), IIR biquads (crossovers), resampling (rubato), MFCC, fluctogram, peak picking.
- `cp-decode`: symphonia-based decoding to f32, plus FLAC seek table and MP3 VBR index generation.
- `cp-beat`:
  - `legacy`: our own DSP tempo / beat tracker (onset envelope, tempo via comb / autocorrelation, dynamic programming beat tracking) for fast "Normal" mode.
  - `nn`: high precision mode; BLSTM (pure Rust, about 100 k parameters, easy to hand-write on `ndarray` or candle) or Beat This! transformer via candle; plus a bar-aware DBN / HMM decoder written from the published papers.
  - `grid`: BPM range folding, first downbeat, dynamic grid segmentation, quantised grid editing operations (the `BeatGridAdjustment` actions as an undoable command list).
- `cp-key`: chroma / HPCP over beat-synchronous frames, template matching with EDM-tuned profiles, candidate list, Camelot mapping.
- `cp-wave`: all waveform kinds (PWAV, PWV2, PWV3, PWV4, PWV5, PWV6, PWV7, PWVC) with exact column rates once section 6 item 8 is answered.
- `cp-phrase`: rule-based structure analyser over the grid (bar energy in bands, kick / snare / hat presence, novelty, 8 / 16 / 32-bar grouping), mood classification, label assignment matching the PSSI vocabulary. Optionally a small learned boundary model later.
- `cp-vocal`: feature front end plus a small BiLSTM / CRNN, or stem-energy based detection when stems exist.
- `cp-loudness`: `ebur128` crate, sample peak, Auto Gain conversion.
- `cp-cue`: candidate generation on phrase boundaries and bar lines, feature extraction, tree-ensemble scorer (our own JSON format, evaluated in Rust), personal trend matrix, score mixing.
- `cp-embed`: tagger CNN, scaler, 2-D projector MLP, energy regressor, genre MLP, neighbour search (brute force cosine is fine below 100 k tracks; `hnsw_rs` or similar beyond).
- `cp-stems`: Demucs (v3 hybrid or v4 htdemucs) on candle with Metal / CPU, chunked with overlap-add; model weights downloaded on first use from our own server, with checksum.

### 7.2 Inference runtime

- Pure Rust only. Small models (BLSTM, MLPs, scalers, trees): hand-written or candle. CNNs exported to ONNX: tract-onnx (check STFT, HannWindow, TreeEnsemble support; if missing, do the STFT in `cp-dsp` and feed spectrograms, which is cleaner anyway). Demucs: candle (LSTM, transformer and conv ops available; Metal backend on macOS).
- Weights stored as safetensors (or tract NNEF), versioned, with a manifest giving model id, training data licence, and checksum. The analysis engine version written to ANLZ / DB includes the model versions so "analysed with old version" works.
- Determinism: fix thread counts and avoid non-deterministic reductions so parity tests are stable.

### 7.3 Training plan for replacements (outline)

1. Beat / downbeat: start with Beat This! (MIT weights) for parity measurement. If its grid behaviour differs from rekordbox in systematic ways (for example first beat choice), add a rekordbox-style post-processor rather than retraining.
2. Vocal: train on MUSDB18 (vocal stem activity as labels; research licence, so only for experiments) and Jamendo Corpus; for shippable weights, use labels derived from our own Demucs vocal stem over CC-BY audio.
3. Tagger / embedding / genre / energy: MTG-Jamendo (commercial-friendly subset) for tagger and genre; AcousticBrainz-derived labels for energy.
4. Cue predictor: needs (track, cue) pairs. Source: opt-in user libraries imported into Cuepoint (rekordbox XML / master.db import gives existing cues). Train a GBDT on features around candidate bars, label = user placed a cue within one beat.

### 7.4 Parity test strategy

Corpus: tracks owned by the team (not committed), plus CC-licensed sets (MTG-Jamendo subset, Free Music Archive CC tracks, GiantSteps key and tempo sets **[verify licence]**). For each track, real rekordbox 7.2.19 analysis in every mode (Normal, Dynamic, high precision on / off, each BPM range used by the corpus), with ANLZ files and the database exported. Keep the ground truth outside the repo; the repo holds only the harness and summary metrics.

Harness (`cargo xtask parity-analysis`): runs our analysis on the same files, parses both ANLZ sets with `cp-anlz`, and reports:

- **BPM:** absolute error; share within 0.01 and 0.1; octave errors separately.
- **Grid:** per-beat offset (ms) after matching beats one-to-one; median and 95th percentile; F-measure at 70 ms (mir_eval convention); downbeat agreement; first-beat (beat 1) agreement.
- **Dynamic grid:** tempo change points matched within 1 beat; per-beat time error.
- **Key:** exact match, MIREX weighted score (fifth 0.5, relative 0.3, parallel 0.2), confusion matrix.
- **Phrase:** boundary precision / recall / F at 1-bar and 2-bar tolerance; label agreement per matched segment; mood agreement.
- **Vocal:** frame accuracy and F1 after resampling both arrays to the same rate.
- **Waveforms:** byte-level diff per tag (exact length and header); per-column mean absolute error per channel; render both to PNG with the same renderer and compare with SSIM, threshold at least 0.98 for preview and 0.95 for detail; also render the CDJ colour variants.
- **Loudness / Auto Gain:** dB difference; peak difference.
- **Cues:** for each rekordbox Intelligent Cue, nearest Cuepoint cue distance in beats; precision / recall within 1 beat; separately with personal trend off (fresh profile) and on (fixed training playlist).
- **Embedding:** genre top-1 agreement after mapping to coarse groups; energy rank correlation (Spearman); neighbour overlap at k = 10 against rekordbox Track Suggestion output captured from the UI.
- **Hardware check:** export a USB with our ANLZ files and load on a CDJ-3000 / XDJ (manual test list), confirming waveform, grid, phrase and vocal display.

Tests run nightly on the private corpus and per commit on a tiny CC-licensed smoke set whose expected values were produced by our own implementation (regression only).
