"""Canonical product identity and visual tokens shared by Mluva assets."""

from typing import Final

PRODUCT_NAME: Final = "Mluva"
PRODUCT_DESCRIPTOR: Final = "Open-source dictation and rewriting for Omarchy."
PRODUCT_VERSION: Final = "1.1.0"
LINUX_USER_AGENT: Final = f"MluvaLinux/{PRODUCT_VERSION}"

BRAND_INK: Final = "#19342F"
BRAND_ACTION: Final = "#287E6B"
BRAND_SURFACE: Final = "#F1F6F3"
BRAND_HIGHLIGHT: Final = "#FFFFFF"
BRAND_SIGNAL: Final = "#91E6C8"

# Static artwork follows the final logo set in docs/brand; application colors remain theme-driven.
LOGO_RED: Final = "#E91B27"  # flat stand-in for the glossy mark
LOGO_INK_ON_DARK: Final = "#F5F5F5"  # default tone
LOGO_INK_ON_LIGHT: Final = "#171717"
LOGO_BACKGROUND: Final = "#000000"  # when a surface needs an opaque background
