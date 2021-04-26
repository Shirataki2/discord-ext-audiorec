.. discord-ext-audiorec documentation master file, created by
   sphinx-quickstart on Mon Apr 26 20:09:30 2021.
   You can adapt this file completely to your liking, but it should at least
   contain the root `toctree` directive.

Welcome to discord-ext-audiorec's documentation!
================================================

discord-ext-audiorec is an extension
package that provides recording feature
to `discord.py <https://discordpy.readthedocs.io/ja/latest/>`_
voice client.


This package works by setting
``cls=NativeVoiceClient`` as an argument
when you connect to a Discord voice channel.


The backend side, such as decoding of voice packets,
is processed by `Rust <https://www.rust-lang.org>`_
with `PyO3 <https://github.com/PyO3/pyo3>`_ crates.

Getting started
===============

.. toctree::
   :maxdepth: 2
   :glob:

   installation.rst
   usage.rst

Manuals
=======

.. toctree::
   :maxdepth: 2

   modules.rst


Indices and tables
==================

* :ref:`genindex`
* :ref:`modindex`
* :ref:`search`
