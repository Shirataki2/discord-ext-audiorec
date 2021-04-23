import os
import asyncio
import pytest
import discord

from discord.ext.audiorec import NativeVoiceClient

@pytest.mark.asyncio
async def test_connect(event_loop: asyncio.AbstractEventLoop) -> None:
    token = os.environ['TOKEN']
    vc = int(os.environ['TESTING_VOICECHANNEL'])
    client = discord.Client(loop=event_loop)
    task = client.loop.create_task(subroutine(client, vc))
    await client.start(token)
    assert task.done()

async def subroutine(client: discord.Client, vc_id: int) -> None:
    await client.wait_until_ready()
    vc = client.get_channel(vc_id)
    assert isinstance(vc, discord.VoiceChannel)
    try:
        if isinstance(vc, discord.VoiceChannel):
            conn = await vc.connect(cls=NativeVoiceClient, timeout=10, reconnect=False)
            await asyncio.sleep(5)
            if conn is not None:
                await conn.disconnect(force=True)
            else:
                assert conn is not None
    except asyncio.TimeoutError:
        raise
    finally:
        await client.close()