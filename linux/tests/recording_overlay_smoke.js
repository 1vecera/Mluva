import Gio from 'gi://Gio';
import GLib from 'gi://GLib';
import Shell from 'gi://Shell';

import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as Scripting from 'resource:///org/gnome/shell/ui/scripting.js';

Gio._promisify(Shell.Screenshot.prototype, 'screenshot');

const BUS_NAME = 'com.voicescribe.Linux';
const OBJECT_PATH = '/com/voicescribe/Linux/RecordingStatus';
const INTERFACE_NAME = 'com.voicescribe.Linux.RecordingStatus';
const SIGNAL_NAME = 'StateChanged';

function expect(condition, message) {
    if (!condition)
        throw new Error(message);
}

function overlayActor() {
    return Main.layoutManager.uiGroup
        .get_children()
        .find(actor => actor.name === 'voiceScribeRecordingOverlay');
}

async function waitFor(predicate, message) {
    for (let attempt = 0; attempt < 40; attempt++) {
        if (predicate())
            return;
        await Scripting.sleep(50);
    }
    throw new Error(message);
}

function ownApplicationName() {
    return new Promise((resolve, reject) => {
        const ownerId = Gio.bus_own_name(
            Gio.BusType.SESSION,
            BUS_NAME,
            Gio.BusNameOwnerFlags.NONE,
            connection => resolve([ownerId, connection]),
            null,
            () => reject(new Error(`Could not own ${BUS_NAME} on the private test bus`)));
    });
}

async function saveStage(path) {
    const stream = Gio.File.new_for_path(path).replace(
        null,
        false,
        Gio.FileCreateFlags.REPLACE_DESTINATION,
        null);
    const shooter = new Shell.Screenshot();
    await shooter.screenshot(false, stream);
    stream.close(null);
}

function visibleState() {
    const scenario = GLib.getenv('VOICE_SCRIBE_OVERLAY_SCENARIO') ?? 'recording';
    if (['processing', 'copied', 'error'].includes(scenario)) {
        const detail = {processing: 'Finishing transcription…', copied: 'Copied. Ready to paste.', error: 'Microphone could not start. Open Mluva.'}[scenario];
        return [true, scenario, detail, 0, '', '', 0, '', ''];
    }
    if (scenario === 'preparing') {
        return [
            true,
            'preparing',
            'Opening microphone',
            0,
            'Dictate',
            'Checking ElevenLabs',
            0,
            'Preparing a private recording session…',
            'Target captured',
        ];
    }
    if (scenario === 'quiet') {
        return [
            true,
            'recording',
            'Listening · no speech yet',
            8,
            'Dictate',
            'ElevenLabs Realtime',
            0.02,
            'Waiting for speech…',
            'Copies when ready',
        ];
    }
    return [
        true,
        'recording',
        'Listening',
        73,
        'Dictate',
        'ElevenLabs Realtime',
        0.68,
        'Your full transcript stays visible in Mluva while recording.',
        'Copies when ready',
    ];
}

export async function run() {
    await Scripting.waitLeisure();
    Main.overview.hide();
    await Scripting.waitLeisure();

    let overlay = overlayActor();
    expect(overlay !== undefined, 'The recording overlay extension did not create its Shell actor');
    expect(!overlay.visible, 'The recording overlay must be absent while idle');
    expect(!overlay.reactive, 'The recording overlay container must not intercept pointer input');

    const focusBefore = global.stage.get_key_focus();
    const [ownerId, connection] = await ownApplicationName();
    const actions = new Gio.SimpleActionGroup();
    const activated = [];
    let currentState = [false, 'hidden', '', 0, '', '', 0, '', ''];
    for (const name of ['record', 'latest', 'history', 'settings', 'quit']) {
        const action = new Gio.SimpleAction({name});
        action.connect('activate', () => activated.push(name));
        actions.add_action(action);
    }
    const statusAction = new Gio.SimpleAction({name: 'status'});
    statusAction.connect('activate', () => connection.emit_signal(
        null, OBJECT_PATH, INTERFACE_NAME, SIGNAL_NAME, new GLib.Variant('(bssussdss)', currentState)));
    actions.add_action(statusAction);
    const exportId = connection.export_action_group('/com/voicescribe/Linux', actions);
    try {
        const indicator = Main.panel.statusArea.mluva;
        expect(indicator !== undefined, 'Mluva has no top-panel menu');
        const items = indicator.menu._getMenuItems();
        await waitFor(() => items.find(item => item.label?.text === 'History')?.sensitive, 'Shell actions did not become ready');
        for (const [label, name] of [['Start dictation', 'record'], ['Rewrite latest dictation', 'latest'], ['History', 'history'], ['Settings', 'settings'], ['Quit Mluva', 'quit']]) {
            const item = items.find(candidate => candidate.label?.text === label);
            item.activate(null);
            await waitFor(() => activated.includes(name), `${label} did not reach the application action group`);
        }
        currentState = visibleState();
        connection.emit_signal(
            null,
            OBJECT_PATH,
            INTERFACE_NAME,
            SIGNAL_NAME,
            new GLib.Variant('(bssussdss)', currentState));
        await waitFor(() => overlay.visible, 'The recording signal did not reveal the overlay');
        await Scripting.waitLeisure();
        if (currentState[1] === 'processing') {
            const extension = Main.extensionManager.lookup('recording-status@voicescribe.local').stateObj;
            extension.disable();
            extension.enable();
            overlay = overlayActor();
            await waitFor(() => overlay.visible, 'Re-enabling the extension did not restore processing status');
            await waitFor(() => Main.panel.statusArea.mluva.accessible_name.includes('Processing'),
                'Re-enabling the extension falsely reported readiness');
            await Scripting.waitLeisure();
        }

        console.log(
            `VOICE_SCRIBE_OVERLAY_GEOMETRY bar=${overlay.width}x${overlay.height} ` +
            `position=${overlay.x},${overlay.y}`);
        expect(global.stage.get_key_focus() === focusBefore, 'The display-only overlay changed keyboard focus');
        expect(overlay.x > 0 && overlay.y > 0, 'The overlay was not positioned inside the virtual monitor');
        expect(overlay.x + overlay.width < 1280, 'The overlay crossed the virtual monitor right edge');
        expect(overlay.y + overlay.height < 720, 'The overlay crossed the virtual monitor bottom edge');

        const screenshotPath = GLib.getenv('VOICE_SCRIBE_OVERLAY_SCREENSHOT');
        expect(Boolean(screenshotPath), 'VOICE_SCRIBE_OVERLAY_SCREENSHOT is required');
        await saveStage(screenshotPath);

        connection.emit_signal(
            null,
            OBJECT_PATH,
            INTERFACE_NAME,
            SIGNAL_NAME,
            new GLib.Variant('(bssussdss)', [false, 'hidden', '', 0, '', '', 0, '', '']));
        await waitFor(() => !overlay.visible, 'The terminal state did not hide the overlay immediately');
    } finally {
        connection.unexport_action_group(exportId);
        Gio.bus_unown_name(ownerId);
    }

    await Scripting.sleep(100);
    expect(!overlay.visible, 'The overlay returned after the application bus name vanished');
}
