// Display-only GNOME Shell projection for Mluva recording state.

import Clutter from 'gi://Clutter';
import Gio from 'gi://Gio';
import Pango from 'gi://Pango';
import St from 'gi://St';

import * as Main from 'resource:///org/gnome/shell/ui/main.js';

const APPLICATION_BUS_NAME = 'com.voicescribe.Linux';
const OBJECT_PATH = '/com/voicescribe/Linux/RecordingStatus';
const INTERFACE_NAME = 'com.voicescribe.Linux.RecordingStatus';
const SIGNAL_NAME = 'StateChanged';
const VISIBLE_PHASES = new Set(['preparing', 'recording']);

export class RecordingOverlay {
    constructor() {
        this._buildUi();
        Main.layoutManager.addChrome(this._container, {
            affectsStruts: false,
            trackFullscreen: false,
        });
        this._monitorsChangedId = Main.layoutManager.connect(
            'monitors-changed', this._position.bind(this));
        this._barAllocationId = this._bar.connect(
            'notify::allocation', this._position.bind(this));
        this._signalId = Gio.DBus.session.signal_subscribe(
            APPLICATION_BUS_NAME,
            INTERFACE_NAME,
            SIGNAL_NAME,
            OBJECT_PATH,
            null,
            Gio.DBusSignalFlags.NONE,
            this._onStateChanged.bind(this));
        this._nameWatchId = Gio.bus_watch_name(
            Gio.BusType.SESSION,
            APPLICATION_BUS_NAME,
            Gio.BusNameWatcherFlags.NONE,
            null,
            this._hide.bind(this));
        this._position();
    }

    destroy() {
        Gio.bus_unwatch_name(this._nameWatchId);
        Gio.DBus.session.signal_unsubscribe(this._signalId);
        Main.layoutManager.disconnect(this._monitorsChangedId);
        this._bar.disconnect(this._barAllocationId);
        this._container.destroy();
        this._waveBars = null;
        this._container = null;
    }

    _buildUi() {
        this._container = new St.BoxLayout({
            name: 'voiceScribeRecordingOverlay',
            style_class: 'mluva-recording-bar',
            orientation: Clutter.Orientation.VERTICAL,
            x_align: Clutter.ActorAlign.START,
            y_align: Clutter.ActorAlign.START,
            x_expand: false,
            y_expand: false,
            reactive: false,
            can_focus: false,
            track_hover: false,
            visible: false,
        });
        this._bar = this._container;

        const primaryRow = new St.BoxLayout({
            style_class: 'mluva-primary-row',
            x_align: Clutter.ActorAlign.CENTER,
            y_align: Clutter.ActorAlign.CENTER,
        });
        this._phaseIcon = new St.Icon({
            icon_name: 'media-record-symbolic',
            style_class: 'mluva-phase-icon',
        });
        primaryRow.add_child(this._phaseIcon);
        this._phaseLabel = new St.Label({
            text: 'PREPARING',
            style_class: 'mluva-phase-label',
            y_align: Clutter.ActorAlign.CENTER,
        });
        primaryRow.add_child(this._phaseLabel);
        this._timeLabel = new St.Label({
            text: '00:00',
            style_class: 'mluva-time',
            y_align: Clutter.ActorAlign.CENTER,
        });
        primaryRow.add_child(this._timeLabel);
        primaryRow.add_child(this._buildWaveform());
        this._modeLabel = new St.Label({
            style_class: 'mluva-chip mluva-mode',
            y_align: Clutter.ActorAlign.CENTER,
        });
        primaryRow.add_child(this._modeLabel);
        this._deliveryLabel = new St.Label({
            style_class: 'mluva-chip mluva-delivery',
            y_align: Clutter.ActorAlign.CENTER,
        });
        primaryRow.add_child(this._deliveryLabel);
        this._bar.add_child(primaryRow);

        this._detailLabel = new St.Label({
            style_class: 'mluva-detail',
            y_align: Clutter.ActorAlign.CENTER,
        });
        this._detailLabel.clutter_text.ellipsize = Pango.EllipsizeMode.END;
        this._detailLabel.clutter_text.single_line_mode = true;
        this._bar.add_child(this._detailLabel);

        this._previewLabel = new St.Label({
            style_class: 'mluva-preview',
            y_align: Clutter.ActorAlign.CENTER,
        });
        this._previewLabel.clutter_text.ellipsize = Pango.EllipsizeMode.END;
        this._previewLabel.clutter_text.single_line_mode = true;
        this._bar.add_child(this._previewLabel);
    }

    _buildWaveform() {
        const waveform = new St.BoxLayout({
            style_class: 'mluva-waveform',
            y_align: Clutter.ActorAlign.CENTER,
        });
        this._waveBars = [];
        for (let index = 0; index < 7; index++) {
            const bar = new St.Widget({
                style_class: 'mluva-wave-bar',
                y_align: Clutter.ActorAlign.CENTER,
                height: 4,
            });
            waveform.add_child(bar);
            this._waveBars.push(bar);
        }
        return waveform;
    }

    _position() {
        const monitor = Main.layoutManager.primaryMonitor;
        if (!monitor)
            return;
        if (this._bar.width > 0 && this._bar.height > 0) {
            const x = monitor.x + Math.max(20, Math.round((monitor.width - this._bar.width) / 2));
            const y = monitor.y + Math.max(20, monitor.height - this._bar.height - 34);
            this._container.set_position(x, y);
        }
    }

    _onStateChanged(_connection, _senderName, _objectPath, _interfaceName, _signalName, parameters) {
        const [visible, phase, detail, elapsed, mode, route, level, preview, delivery] =
            parameters.deepUnpack();
        if (!visible || !VISIBLE_PHASES.has(phase)) {
            this._hide();
            return;
        }
        const safeLevel = Number.isFinite(level) ? Math.max(0, Math.min(level, 1)) : 0;
        this._phaseLabel.text = phase === 'preparing' ? 'PREPARING' : 'RECORDING';
        this._phaseIcon.style_class = phase === 'preparing'
            ? 'mluva-phase-icon mluva-preparing'
            : 'mluva-phase-icon';
        this._timeLabel.text = this._formatElapsed(elapsed);
        this._modeLabel.text = this._bounded(mode, 32);
        this._modeLabel.visible = this._modeLabel.text.length > 0;
        this._deliveryLabel.text = this._bounded(delivery, 48);
        this._deliveryLabel.visible = this._deliveryLabel.text.length > 0;
        const routeText = this._bounded(route, 48);
        const detailText = this._bounded(detail, 80);
        this._detailLabel.text = [detailText, routeText].filter(Boolean).join(' · ');
        this._detailLabel.visible = this._detailLabel.text.length > 0;
        this._previewLabel.text = this._bounded(preview, 180);
        this._previewLabel.visible = this._previewLabel.text.length > 0;
        this._updateWaveform(safeLevel);
        this._container.show();
    }

    _updateWaveform(level) {
        const shape = [0.38, 0.72, 1, 0.58, 0.84, 0.5, 0.3];
        for (let index = 0; index < this._waveBars.length; index++)
            this._waveBars[index].height = Math.round(4 + 18 * level * shape[index]);
    }

    _formatElapsed(seconds) {
        const bounded = Math.max(0, Math.min(Math.trunc(seconds), 86400));
        const minutes = Math.floor(bounded / 60);
        return `${String(minutes).padStart(2, '0')}:${String(bounded % 60).padStart(2, '0')}`;
    }

    _bounded(value, limit) {
        return String(value).replace(/\s+/g, ' ').trim().slice(0, limit);
    }

    _hide() {
        this._phaseLabel.text = '';
        this._timeLabel.text = '00:00';
        this._modeLabel.text = '';
        this._deliveryLabel.text = '';
        this._detailLabel.text = '';
        this._previewLabel.text = '';
        this._updateWaveform(0);
        this._container.hide();
    }
}
