from .voice_client import NativeVoiceClient

# Version settings

from collections import namedtuple

VersionInfo = namedtuple('VersionInfo', 'major minor micro releaselevel serial')
version_info = VersionInfo(major=0, minor=1, micro=2, releaselevel='alpha', serial=0)

__version__ = '.'.join(map(str, [version_info.major, version_info.minor, version_info.micro]))
