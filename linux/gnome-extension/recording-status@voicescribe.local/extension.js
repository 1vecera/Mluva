// Display-only GNOME Shell projection for Mluva recording state.

import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';

import {RecordingOverlay} from './recordingOverlay.js';

export default class MluvaRecordingStatusExtension extends Extension {
    enable() {
        this._overlay = new RecordingOverlay();
    }

    disable() {
        this._overlay.destroy();
        this._overlay = null;
    }
}
