import asyncio
import discord
import logging
from typing import Dict, Optional

from discord.voice_client import VoiceProtocol
from discord.client import Client
from discord.channel import VoiceChannel
from discord.backoff import ExponentialBackoff

from .ffi import VoiceConnector, VoiceConnection
from . import ffi

log = logging.getLogger(__name__)

class NativeVoiceClient(VoiceProtocol):
    def __init__(self, client: Client, channel: VoiceChannel) -> None:
        super().__init__(client, channel)
        self.connector = VoiceConnector()
        self.connector.user_id = str(client.user.id)
        self.connection: Optional[VoiceConnection] = None
        self.guild = channel.guild
        self._attempts = 0
        self._runner: Optional[asyncio.Task] = None
        self.voice_state_received = asyncio.Event()
        self.voice_server_received = asyncio.Event()

    async def on_voice_state_update(self, data: dict) -> None:
        session_id = data['session_id']
        log.info('Voice Session ID: %s', session_id)
        self.connector.session_id = session_id
        # すでに接続が確立している場合
        if self.connection is not None:
            channel_id = data['channel_id']
            if channel_id is None:
                return await self.disconnect()
            else:
                self.channel = self.guild.get_channel(int(channel_id))
        else:
            self.voice_state_received.set()

    async def on_voice_server_update(self, data: dict) -> None:
        if self.voice_server_received.is_set():
            log.info('Ignore extraneous voice server update')
            return
        server_id = data['guild_id']
        token: Optional[str] = data.get('token')
        endpoint: Optional[str] = data.get('endpoint')
        if endpoint is None or token is None:
            log.warning('Awaiting endpoint... This requires waiting.')
            return
        log.info('Voice Gateway Endpoint: %s', endpoint)
        # [host, ':', port]
        endpoint, _, _ = endpoint.rpartition(':')
        if endpoint.startswith('wss://'):
            endpoint = endpoint[6:]
        self.connector.update_connection_config(token, server_id, endpoint)
        self.voice_server_received.set()

    async def connect(self, *, reconnect: bool, timeout: float) -> None:
        log.info('Connecting to voice channel')
        self.voice_server_received.clear()
        self.voice_state_received.clear()
        futures = [
            self.voice_server_received.wait(),
            self.voice_state_received.wait()
        ]
        await self.voice_connect()

        try:
            await discord.utils.sane_wait_for(futures, timeout=timeout)
        except asyncio.TimeoutError:
            await self.disconnect(force=True)
            raise
        self.voice_server_received.clear()
        self.voice_state_received.clear()
        loop = asyncio.get_running_loop()
        self.connection = await self.connector.connect(loop)
        if self._runner is not None:
            self._runner.cancel()

        self._runner = loop.create_task(self.reconnect_handler(reconnect, timeout))

    async def disconnect(self, *, force: bool = False) -> None:
        try:
            if self.connection is not None:
                self.connection.disconnect()
                self.connection = None
            await self.voice_disconnect()
        finally:
            self.cleanup()

    async def voice_connect(self):
        self._attempts += 1
        await self.guild.change_voice_state(channel=self.channel)

    async def voice_disconnect(self):
        log.info('The voice handshake is being terminated for Channel ID %s (Guild ID %s)', self.channel.id, self.guild.id)
        await self.guild.change_voice_state(channel=None)

    def play(self, input: str):
        if self.connection:
            self.connection.play(input)
    
    def stop(self):
        if self.connection:
            self.connection.stop()

    def is_playing(self) -> bool:
        if self.connection:
            return self.connection.is_playing()
        return False

    def is_recording(self) -> bool:
        if self.connection:
            return self.connection.is_recording()
        return False

    def record(self) -> None:
        if self.connection:
            return self.connection.record()

    async def stop_record(self, loop_: Optional[asyncio.AbstractEventLoop] = None) -> Optional[bytes]:
        if self.connection:
            if loop_ is None:
                loop_ = asyncio.get_event_loop()
            return await self.connection.stop_record(loop_)
        return None

    def get_state(self) -> Dict:
        if self.connection:
            return self.connection.get_state()
        return {}

    async def reconnect_handler(self, reconnect, timeout):
        backoff = ExponentialBackoff()
        loop = asyncio.get_running_loop()

        while True:
            try:
                await self.connection.run(loop)
            except ffi.GatewayError as e:
                log.info('Voice connection got a clean close %s', e)
                await self.disconnect()
                return
            except (ffi.TryReconnect) as e:
                if not reconnect:
                    await self.disconnect()
                    raise

                retry = backoff.delay()
                log.exception('Disconnected from voice... Reconnecting in %.2fs.', retry)

                await asyncio.sleep(retry)
                await self.voice_disconnect()
                try:
                    await self.connect(reconnect=True, timeout=timeout)
                except asyncio.TimeoutError:
                    log.warning('Could not connect to voice... Retrying...')
                    continue
            else:
                await self.disconnect()
                return

    def debug(self):
        print(self.connection.get_state())
