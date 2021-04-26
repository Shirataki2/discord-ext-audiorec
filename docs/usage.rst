Usage
=====

This voice implementation operates
using the discord.py ``VoiceProtocol`` interface.

To use it, pass the class into
`VoiceChannel.connect <https://discordpy.readthedocs.io/en/latest/api.html#discord.VoiceChannel.connect>`_

.. code-block:: python3

    from discord.ext.audiorec import NativeVoiceClient

    # ...
    client = await voice_channel.connect(cls=NativeVoiceClient)

    client.record()

    await asyncio.sleep(30)

    audio_binaries = await client.stop_record()


For other examples, please see the
`examples folder <https://github.com/Shirataki2/discord-ext-audiorec>`_
on GitHub.
