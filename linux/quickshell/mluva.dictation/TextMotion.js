.pragma library

// Offset-preserving word diffs. The bounded preview never needs a general diff service.
function changes(before, after) {
    let prefix = 0;
    while (prefix < before.length && prefix < after.length && before[prefix] === after[prefix]) prefix++;
    // JavaScript offsets are UTF-16. Never insert markup between an emoji's surrogate pair.
    function splitsPair(text, offset) {
        return offset > 0 && offset < text.length
            && text.charCodeAt(offset - 1) >= 0xd800 && text.charCodeAt(offset - 1) <= 0xdbff
            && text.charCodeAt(offset) >= 0xdc00 && text.charCodeAt(offset) <= 0xdfff;
    }
    if (splitsPair(before, prefix) || splitsPair(after, prefix)) prefix--;
    let suffix = 0;
    while (suffix < Math.min(before.length, after.length) - prefix
        && before[before.length - suffix - 1] === after[after.length - suffix - 1]) suffix++;
    if (splitsPair(before, before.length - suffix) || splitsPair(after, after.length - suffix)) suffix--;
    if (prefix === before.length && prefix === after.length) return [];
    const oldEnd = before.length - suffix, newEnd = after.length - suffix;
    const oldWords = before.slice(prefix, oldEnd).match(/\s+|[^\s]+/g) || [];
    const newWords = after.slice(prefix, newEnd).match(/\s+|[^\s]+/g) || [];
    if (oldWords.length * newWords.length > 90000) return [[prefix, oldEnd, prefix, newEnd]];
    const oldOffsets = [prefix], newOffsets = [prefix];
    oldWords.forEach(word => oldOffsets.push(oldOffsets[oldOffsets.length - 1] + word.length));
    newWords.forEach(word => newOffsets.push(newOffsets[newOffsets.length - 1] + word.length));
    const rows = Array.from({length: oldWords.length + 1}, () => Array(newWords.length + 1).fill(0));
    for (let i = oldWords.length - 1; i >= 0; i--)
        for (let j = newWords.length - 1; j >= 0; j--)
            rows[i][j] = oldWords[i] === newWords[j] ? rows[i + 1][j + 1] + 1 : Math.max(rows[i + 1][j], rows[i][j + 1]);
    const result = [];
    let i = 0, j = 0, startI = 0, startJ = 0;
    function flush() {
        if (startI !== i || startJ !== j) result.push([oldOffsets[startI], oldOffsets[i], newOffsets[startJ], newOffsets[j]]);
    }
    while (i < oldWords.length || j < newWords.length) {
        if (i < oldWords.length && j < newWords.length && oldWords[i] === newWords[j]) {
            flush(); i++; j++; startI = i; startJ = j;
        } else if (j < newWords.length && (i === oldWords.length || rows[i][j + 1] >= rows[i + 1][j])) j++;
        else i++;
    }
    flush();
    return result;
}

function escapeText(text) {
    return text.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;")
        .replace(/\n/g, "<br>").replace(/ {2,}/g, spaces => " " + "&#160;".repeat(spaces.length - 1));
}

function styled(text, differences, old, alpha, ink) {
    const rgb = [ink.r, ink.g, ink.b].map(value => Math.round(value * 255).toString(16).padStart(2, "0")).join("");
    const changed = "#" + Math.round(Math.max(0, Math.min(1, alpha)) * 255).toString(16).padStart(2, "0") + rgb;
    const stable = old ? "#00" + rgb : "#ff" + rgb;
    let html = "", cursor = 0;
    for (const diff of differences) {
        const start = diff[old ? 0 : 2], end = diff[old ? 1 : 3];
        html += '<font color="' + stable + '">' + escapeText(text.slice(cursor, start)) + "</font>";
        html += '<font color="' + changed + '">' + escapeText(text.slice(start, end)) + "</font>";
        cursor = end;
    }
    return html + '<font color="' + stable + '">' + escapeText(text.slice(cursor)) + "</font>";
}

function forecast(samples, columns, fill, horizon, limit) {
    let rate = 12;
    if (samples.length > 1) {
        const elapsed = (samples[samples.length - 1][0] - samples[0][0]) / 1000;
        if (elapsed >= 0.25) rate = Math.min(80, Math.max(0, (samples[samples.length - 1][1] - samples[0][1]) / elapsed));
    }
    return Math.min(Math.max(0, limit), Math.max(0, fill + rate * Math.max(0.2, horizon) / Math.max(1, columns) - 1));
}
