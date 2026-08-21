import Testing
@testable import VoiceScribeMac

@Suite("Permissions Manager")
struct PermissionsManagerTests {

    @Test("allGranted is false when microphone is denied")
    func micDeniedMeansNotAllGranted() {
        let pm = PermissionsManager()
        // In test environment, neither permission is likely granted
        // But allGranted requires BOTH to be true
        if !pm.microphoneGranted {
            #expect(pm.allGranted == false)
        }
    }

    @Test("allGranted is false when accessibility is denied")
    func accessibilityDeniedMeansNotAllGranted() {
        let pm = PermissionsManager()
        if !pm.accessibilityGranted {
            #expect(pm.allGranted == false)
        }
    }

    @Test("canRecord depends only on microphone, not accessibility")
    func canRecordDependsOnlyOnMicrophone() {
        let pm = PermissionsManager()
        // canRecord should equal microphoneGranted regardless of accessibility
        #expect(pm.canRecord == pm.microphoneGranted)
    }

    @Test("Meeting capture requires microphone and Screen Recording")
    func meetingCapturePermissionBoundary() {
        let pm = PermissionsManager()

        #expect(
            pm.canCaptureMeeting
                == (pm.microphoneGranted && pm.screenRecordingGranted)
        )
    }

    @Test("canRecord is true when microphone is granted even without accessibility")
    func canRecordWithoutAccessibility() {
        let pm = PermissionsManager()
        // We can't control actual TCC state, but we can verify the property logic
        // canRecord is defined as just microphoneGranted
        if pm.microphoneGranted && !pm.accessibilityGranted {
            #expect(pm.canRecord == true)
            #expect(pm.allGranted == false)
        }
    }

    @Test("checkAll does not crash")
    func checkAllNoCrash() {
        let pm = PermissionsManager()
        pm.checkAll()
        // Just verifying it doesn't crash
        _ = pm.microphoneGranted
        _ = pm.accessibilityGranted
        _ = pm.allGranted
        _ = pm.canRecord
    }

    @Test("startPolling and stopPolling are safe")
    func pollingLifecycle() {
        let pm = PermissionsManager()
        pm.startPolling()
        pm.startPolling() // double-start should be safe
        pm.stopPolling()
        pm.stopPolling() // double-stop should be safe
    }

    @Test("Permission polling is idle until an interactive permission flow starts")
    func pollingDoesNotRunForeverInProviderSpecificModes() {
        let pm = PermissionsManager()

        #expect(pm.isPolling == false)
    }

    @Test("Google recognition does not require Apple Speech permission")
    func googleDoesNotRequireAppleSpeech() {
        #expect(
            RecognitionPermissionPolicy.missingRequirement(
                preference: .googleCloud,
                microphoneGranted: true,
                appleSpeechGranted: false,
                cloudFallbackAvailable: true
            ) == nil
        )
    }

    @Test("Automatic recognition can use an explicitly permitted cloud fallback")
    func automaticCanUseCloudWithoutAppleSpeech() {
        #expect(
            RecognitionPermissionPolicy.missingRequirement(
                preference: .automatic,
                microphoneGranted: true,
                appleSpeechGranted: false,
                cloudFallbackAvailable: true
            ) == nil
        )
    }

    @Test("Apple recognition reports its missing Speech permission")
    func appleReportsMissingSpeechPermission() {
        #expect(
            RecognitionPermissionPolicy.missingRequirement(
                preference: .apple,
                microphoneGranted: true,
                appleSpeechGranted: false,
                cloudFallbackAvailable: true
            ) == .appleSpeech
        )
    }

    @Test("Every provider requires microphone permission")
    func everyProviderRequiresMicrophone() {
        #expect(
            RecognitionPermissionPolicy.missingRequirement(
                preference: .googleCloud,
                microphoneGranted: false,
                appleSpeechGranted: true,
                cloudFallbackAvailable: true
            ) == .microphone
        )
    }
}
