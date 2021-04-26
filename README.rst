discord-ext-audiorec
####################

|Docs| |PyPI| |Support|


**This project is currently under development. We do not guarantee it works.**

A discord.py experimental extension for audio recording

Inspired by `discord-ext-native-voice <https://github.com/Rapptz/discord-ext-native-voice>`_

Installation
============

Supported OS: Windows, Mac OS X, Linux
Supported Python Version: 3.6, 3.7, 3.8, 3.9

.. code-block:: sh

    python -m pip install -U discord-ext-audiorec


Build
=====

Requires
++++++++

- Rust 1.47 +

.. code-block:: sh

    python -m pip install -r requirements-dev.txt

    python -m pip install -U .
    # or
    python setup.py develop

The compilation and Rust package resolution will
be automatically handled by setuptools-rust.


.. |Docs| image:: https://readthedocs.org/projects/discord-ext-audiorec/badge/?version=latest
    :target: https://discord-ext-audiorec.readthedocs.io/en/latest/?badge=latest

.. |PyPI| image:: https://badge.fury.io/py/discord-ext-audiorec.svg
    :target: https://pypi.org/project/discord-ext-audiorec/


.. |Support| image:: https://img.shields.io/pypi/pyversions/discord-ext-audiorec.svg
    :target: https://pypi.org/project/discord-ext-audiorec/
