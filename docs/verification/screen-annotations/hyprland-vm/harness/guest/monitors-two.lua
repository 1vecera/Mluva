-- Two outputs: Virtual-1 at scale 1, Virtual-2 to its right at fractional scale 1.5.
hl.monitor({ output = "Virtual-1", mode = "1920x1080", position = "0x0", scale = 1 })
hl.monitor({ output = "Virtual-2", mode = "1920x1080", position = "1920x0", scale = 1.5 })
