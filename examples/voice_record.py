import asyncio
import os
import io

import discord
import logging

from discord.ext import commands
from discord.ext.audiorec import NativeVoiceClient

logging.basicConfig(level=logging.INFO)

class Recorder(commands.Cog):
    def __init__(self, bot: commands.Bot):
        self.bot = bot

    @commands.command()
    async def join(self, ctx: commands.Context):
        """Joins a voice channel"""

        channel: discord.VoiceChannel = ctx.author.voice.channel # type: ignore
        if ctx.voice_client is not None:
            return await ctx.voice_client.move_to(channel)

        await channel.connect(cls=NativeVoiceClient)

    @commands.command()
    async def rec(self, ctx: commands.Context):
        """Start recording"""
        ctx.voice_client.record(lambda e: print(f"Exception: {e}"))
        
        await ctx.send(f'Start Recording')

        await asyncio.sleep(30)

        await ctx.invoke(self.bot.get_command('stop'))

    @commands.command()
    async def stop(self, ctx: commands.Context):
        """Stops and disconnects the bot from voice"""
        await ctx.send(f'Stop Recording')

        wav_bytes = await ctx.voice_client.stop_record()

        wav_file = discord.File(io.BytesIO(wav_bytes), filename="Recorded.wav")

        await ctx.send(file=wav_file)


    @rec.before_invoke
    async def ensure_voice(self, ctx: commands.Context):
        if ctx.voice_client is None:
            if ctx.author.voice: # type: ignore
                await ctx.author.voice.channel.connect(cls=NativeVoiceClient) # type: ignore
            else:
                await ctx.send("You are not connected to a voice channel.")
                raise commands.CommandError("Author not connected to a voice channel.")
        elif ctx.voice_client.is_playing():
            ctx.voice_client.stop()

bot = commands.Bot(command_prefix=commands.when_mentioned_or("+"),
                   description='Relatively simple recording bot example')

@bot.event
async def on_ready():
    print('Logged in as')
    print(bot.user.name)
    print(bot.user.id)
    print('------')

bot.add_cog(Recorder(bot))
bot.run(os.environ['TOKEN'])
