#!/usr/bin/env swift
//
// orbix-speak - AVSpeechSynthesizer bridge for OrbixCore
//
// Usage: orbix-speak <voice-id> <rate> <text>
//   voice-id: AVSpeechSynthesisVoice identifier
//             e.g. "com.apple.voice.premium.en-US.Zoe"
//             or "auto" to pick the best available
//   rate:     0.0-1.0 (0.5 = default, maps to AVSpeechUtteranceDefaultSpeechRate)
//   text:     the text to speak (can contain spaces)
//
// The process exits when speech finishes or is interrupted.
// Killing the process stops playback immediately.
//

import AVFoundation
import Foundation

// ── Pick the best voice ────────────────────────────────
func bestVoice(requested: String) -> AVSpeechSynthesisVoice? {
    // If a specific voice was requested, try it
    if requested != "auto" {
        if let voice = AVSpeechSynthesisVoice(identifier: requested) {
            return voice
        }
        // Try by name (e.g. "Zoe", "Ava", "Samantha")
        let allVoices = AVSpeechSynthesisVoice.speechVoices()
        if let byName = allVoices.first(where: { $0.name.lowercased() == requested.lowercased() && $0.language.starts(with: "en") }) {
            return byName
        }
    }

    // Auto-pick: prefer premium > enhanced > default, English only
    let english = AVSpeechSynthesisVoice.speechVoices().filter { $0.language.starts(with: "en") }

    // Premium voices (best quality - Siri-tier)
    let premium = english.filter { $0.quality == .premium }
    if let voice = premium.first { return voice }

    // Enhanced voices (good quality - downloaded neural)
    let enhanced = english.filter { $0.quality == .enhanced }
    if let voice = enhanced.first { return voice }

    // Default - always present
    return AVSpeechSynthesisVoice(language: "en-US")
}

// ── Delegate to track completion ───────────────────────
class SpeechDelegate: NSObject, AVSpeechSynthesizerDelegate {
    let semaphore = DispatchSemaphore(value: 0)

    func speechSynthesizer(_ synthesizer: AVSpeechSynthesizer, didFinish utterance: AVSpeechUtterance) {
        semaphore.signal()
    }

    func speechSynthesizer(_ synthesizer: AVSpeechSynthesizer, didCancel utterance: AVSpeechUtterance) {
        semaphore.signal()
    }
}

// ── Main ───────────────────────────────────────────────
guard CommandLine.arguments.count >= 4 else {
    fputs("Usage: orbix-speak <voice-id|auto> <rate> <text...>\n", stderr)
    exit(1)
}

let voiceArg = CommandLine.arguments[1]
let rateArg = Float(CommandLine.arguments[2]) ?? 0.5
let text = CommandLine.arguments[3...].joined(separator: " ")

guard !text.isEmpty else { exit(0) }

guard let voice = bestVoice(requested: voiceArg) else {
    fputs("[orbix-speak] No English voice available\n", stderr)
    exit(1)
}

fputs("[orbix-speak] Using voice: \(voice.name) (\(voice.identifier)) quality=\(voice.quality.rawValue)\n", stderr)

let utterance = AVSpeechUtterance(string: text)
utterance.voice = voice
// Rate: AVSpeech uses 0.0-1.0 where 0.5 is default speaking rate
utterance.rate = max(0.35, min(0.65, rateArg))
utterance.pitchMultiplier = 1.0
utterance.preUtteranceDelay = 0.0
utterance.postUtteranceDelay = 0.0

let synth = AVSpeechSynthesizer()
let delegate = SpeechDelegate()
synth.delegate = delegate

// Handle SIGTERM/SIGINT gracefully - stop speaking and exit
signal(SIGTERM) { _ in
    // Can't call synth.stopSpeaking here directly (not signal-safe),
    // but exiting the process will stop audio playback.
    exit(0)
}
signal(SIGINT) { _ in
    exit(0)
}

synth.speak(utterance)

// Block until speech finishes
delegate.semaphore.wait()
