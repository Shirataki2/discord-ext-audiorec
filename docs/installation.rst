============
Installation
============

Using wheel
===========

Requires
++++++++

- pip
- python >= 3.6

.. code-block:: sh

    # Windows
    py -3 -m pip install --upgrade discord-ext-audiorec

    # Linux / MacOS
    python3 -m pip install --upgrade discord-ext-audiorec

Build From Source
=================

Requires
++++++++

- pip
- libopus
- Rust >= 1.47.0
- Cargo and rust crates in Cargo.toml
- Python packages in requirements.txt and requirements-dev.txt

.. code-block:: sh

    pip install -U -r requirements-dev.txt
    pip install -U -r requirements.txt
    pip install -U .

The compilation of Rust libraries is done
automatically by ``setuptools_rust``.

