FROM registry.fedoraproject.org/fedora@sha256:f59ce614aba37211165e711d732fd4785f1c0329c4a3a4289cf29e333cf36c52

RUN dnf -y --setopt=install_weak_deps=False install \
    at-spi2-core dbus-daemon git gobject-introspection gtk4 libadwaita make \
    pipewire-utils procps-ng python3-gobject shellcheck uv wl-clipboard \
    quickshell xorg-x11-server-Xvfb xorg-x11-xauth xdotool ImageMagick \
    ffmpeg-free jq dejavu-sans-fonts google-noto-sans-fonts \
    && dnf clean all

ARG OMARCHY_COMMIT=e848f1df97bbbe23db42fc2b1fb04937d7a0eae0
RUN git init /usr/share/omarchy \
    && git -C /usr/share/omarchy remote add origin https://github.com/omacom/omarchy.git \
    && git -C /usr/share/omarchy fetch --depth=1 origin "$OMARCHY_COMMIT" \
    && git -C /usr/share/omarchy checkout --detach FETCH_HEAD

RUN dnf -y --setopt=install_weak_deps=False install picom ripgrep rsms-inter-fonts \
    && dnf clean all

RUN useradd --create-home --uid 1000 developer
USER developer
WORKDIR /workspace
ENV UV_PYTHON_DOWNLOADS=never UV_LINK_MODE=copy QT_QPA_PLATFORM=xcb GSK_RENDERER=cairo
CMD ["sleep", "infinity"]
