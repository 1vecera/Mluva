// Shell controls delegate explicit actions to Mluva; the bottom bar never takes focus.

import Gio from 'gi://Gio';
import St from 'gi://St';
import {Extension} from 'resource:///org/gnome/shell/extensions/extension.js';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';
import * as PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';

import {RecordingOverlay} from './recordingOverlay.js';

export default class MluvaRecordingStatusExtension extends Extension {
    enable() {
        this._indicator = new PanelMenu.Button(0.0, 'Mluva');
        this._icon = new St.Icon({
            gicon: new Gio.FileIcon({file: Gio.File.new_for_path(`${this.path}/mluva-symbolic.svg`)}),
            style_class: 'system-status-icon',
        });
        this._indicator.add_child(this._icon);
        this._status = new PopupMenu.PopupMenuItem('Mluva is not running', {reactive: false});
        this._indicator.menu.addMenuItem(this._status);
        this._indicator.menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());
        this._actions = Gio.DBusActionGroup.get(Gio.DBus.session, 'com.mluva.Linux', '/com/mluva/Linux');
        this._actionAdded = this._actions.connect('action-added', (_group, name) => {
            if (name === 'status')
                this._actions.activate_action('status', null);
        });
        this._record = this._indicator.menu.addAction('Start dictation', () => this._actions.activate_action('record', null));
        this._latest = this._indicator.menu.addAction('Rewrite latest dictation', () => this._actions.activate_action('latest', null));
        this._history = this._indicator.menu.addAction('History', () => this._actions.activate_action('history', null));
        this._indicator.menu.addAction('Open Mluva', () => {
            const app = Gio.DesktopAppInfo.new('com.mluva.Linux.desktop');
            app?.launch([], global.create_app_launch_context(0, -1));
        });
        this._indicator.menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());
        this._settings = this._indicator.menu.addAction('Settings', () => this._actions.activate_action('settings', null));
        this._quit = this._indicator.menu.addAction('Quit Mluva', () => this._actions.activate_action('quit', null));
        Main.panel.addToStatusArea('mluva', this._indicator);
        this._watch = Gio.bus_watch_name(
            Gio.BusType.SESSION, 'com.mluva.Linux', Gio.BusNameWatcherFlags.NONE,
            () => this._setRunning(true), () => this._setRunning(false));
        this._overlay = new RecordingOverlay((phase, detail) => this._setPhase(phase, detail));
    }

    _setRunning(running) {
        this._running = running;
        for (const item of [this._record, this._latest, this._history, this._settings, this._quit])
            item.setSensitive(running);
        this._setPhase('hidden');
        if (running && this._actions.has_action('status'))
            this._actions.activate_action('status', null);
    }

    _setPhase(phase, detail = '') {
        const labels = {
            preparing: 'Preparing microphone…', recording: 'Recording', processing: 'Processing…',
            copied: detail || 'Text copied', error: 'Dictation needs attention', hidden: 'Ready to dictate',
        };
        this._status.label.text = this._running ? labels[phase] : 'Mluva is not running';
        this._record.label.text = phase === 'recording' ? 'Stop dictation' :
            phase === 'preparing' ? 'Cancel preparation' : 'Start dictation';
        this._record.setSensitive(Boolean(this._running) && phase !== 'processing');
        this._icon.style_class = `system-status-icon mluva-panel-${phase}`;
        this._indicator.accessible_name = `Mluva: ${this._status.label.text}`;
    }

    disable() {
        this._actions.disconnect(this._actionAdded);
        Gio.bus_unwatch_name(this._watch);
        this._overlay.destroy();
        this._overlay = null;
        this._indicator.destroy();
        this._indicator = null;
        this._actions = null;
    }
}
