M=$HOME/film/tools
rm -f $M/pw-record $M/raw.ready.json
cat > $M/pw-record <<W
#!/bin/sh
rm -f $M/raw.ready.json
exec $M/audio-fixture-peer "\$@"
W
chmod 755 $M/pw-record
