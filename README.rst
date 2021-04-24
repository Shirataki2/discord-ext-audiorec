discord-ext-audiorec
####################

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

Usage
=====

.. code-block:: python

    import os
    import io

    import discord
    import logging

    from discord.ext import commands
    from discord.ext.audiorec import NativeVoiceClient

    logging.basicConfig(level=logging.INFO)

    class Music(commands.Cog):
        def __init__(self, bot):
            self.bot = bot

        @commands.command()
        async def join(self, ctx: commands.Context):
            """Joins a voice channel"""

            channel: discord.VoiceChannel = ctx.author.voice.channel # type: ignore
            if ctx.voice_client is not None:
                return await ctx.voice_client.move_to(channel)

            await channel.connect(cls=NativeVoiceClient)

        @commands.command()
        async def rec(self, ctx):
            """Start recording"""

            ctx.voice_client.record()

            await ctx.send(f'Start Recording')

        @commands.command()
        async def stop(self, ctx: commands.Context):
            """Stops and disconnects the bot from voice"""

            wav_bytes = await ctx.voice_client.stop_record()

            wav_file = discord.File(io.BytesIO(wav_bytes), filename="Recorded.wav")

            await ctx.send(file=wav_file)



        @rec.before_invoke
        async def ensure_voice(self, ctx):
            if ctx.voice_client is None:
                if ctx.author.voice:
                    await ctx.author.voice.channel.connect(cls=NativeVoiceClient)
                else:
                    await ctx.send("You are not connected to a voice channel.")
                    raise commands.CommandError("Author not connected to a voice channel.")
            elif ctx.voice_client.is_playing():
                ctx.voice_client.stop()

    bot = commands.Bot(command_prefix=commands.when_mentioned_or("+"),
                    description='Relatively simple music bot example')

    @bot.event
    async def on_ready():
        print('Logged in as')
        print(bot.user.name)
        print(bot.user.id)
        print('------')

    bot.add_cog(Music(bot))
    bot.run(os.environ['TOKEN'])
