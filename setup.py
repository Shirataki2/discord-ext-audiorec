import re
import os
from pathlib import Path
from setuptools import setup

kwargs = {}

IS_READTHEDOCS = os.environ.get("IS_READTHEDOCS")

if not IS_READTHEDOCS:
    from setuptools_rust import RustExtension
    kwargs.update({"rust_extensions": [RustExtension('discord.ext.audiorec.ffi')]})

CURDIR = Path(os.path.dirname(__file__))
PROJECT_ROOT = CURDIR / 'discord' / 'ext' / 'audiorec'

with open(PROJECT_ROOT / '__init__.py', encoding='utf-8') as f:
    VERSION_MATCH = re.search(
        r'VersionInfo\(major\s*?=\s*?(\d+)?,\s*?minor\s*?=\s*?(\d+)?,\s*?micro\s*?=\s*?(\d+)?,.*?\)',
        f.read(),
        re.MULTILINE
    )

if not VERSION_MATCH:
    raise RuntimeError('VersionInfo not found')

VERSION = '.'.join([VERSION_MATCH.group(i) for i in range(1, 4)])

with open(CURDIR / 'README.rst', encoding='utf-8') as f:
    LONG_DESCRIPTION = f.read()

CLASSIFIERS = [
    'Development Status :: 3 - Alpha',
    'Intended Audience :: Developers',
    'License :: OSI Approved :: MIT License',
    'Natural Language :: English',
    'Operating System :: OS Independent',
    'Programming Language :: Python :: 3',
    'Programming Language :: Python :: 3 :: Only',
    'Programming Language :: Python :: 3.6',
    'Programming Language :: Python :: 3.7',
    'Programming Language :: Python :: 3.8',
    'Programming Language :: Python :: 3.9',
    'Programming Language :: Python :: Implementation :: CPython',
    'Topic :: Software Development',
    'Topic :: Software Development :: Libraries :: Python Modules'
]

with open(CURDIR / 'requirements.txt', encoding='utf-8') as f:
    REQUIRES = f.read().splitlines()

with open(CURDIR / 'requirements-dev.txt', encoding='utf-8') as f:
    SETUP_REQUIRES = f.read().splitlines()

SETUP_REQUIRES.append('wheel')

EXTRA_REQUIRES = {
    'docs': [
        'sphinx',
        'sphinxcontrib_trio',
        'sphinxcontrib-websupport',
        'sphinx-rtd-theme',
    ],
}

setup(
    name='discord-ext-audiorec',
    author='Shirataki2',
    license='MIT',
    author_email='tmy1997530@icloud.com',
    classifiers=CLASSIFIERS,
    description='A discord.py experimental extention for audio recording',
    long_description=LONG_DESCRIPTION,
    long_description_content_type='text/x-rst',
    install_requires=REQUIRES,
    extras_require=EXTRA_REQUIRES,
    setup_requires=SETUP_REQUIRES,
    packages=['discord.ext.audiorec'],
    python_requires='>=3.6.0',
    url='https://github.com/shirataki2/discord-ext-audiorec',
    version=VERSION,
    include_package_data=True,
    zip_safe=False,
    **kwargs
)
