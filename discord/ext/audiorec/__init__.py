"""
Discord Audio Record Extension
~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~

:copyright: (c) 2021 Tomoya Ishii
:license: MIT
"""

from .voice_client import NativeVoiceClient

__title__ = 'discord-ext-audiorec'
__author__ = 'Tomoya Ishii'
__license__ = 'MIT'
__copyright__ = 'Copyright 2021-present Tomoya Ishii'

# Version settings

from collections import namedtuple

VersionInfo = namedtuple('VersionInfo', 'major minor micro releaselevel serial')
version_info = VersionInfo(major=0, minor=1, micro=6, releaselevel='alpha', serial=0)

__version__ = '.'.join(map(str, [version_info.major, version_info.minor, version_info.micro]))
