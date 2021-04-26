import asyncio
import discord
import logging
from typing import Callable, Dict, Optional

from discord.voice_client import VoiceProtocol
from discord.client import Client
from discord.channel import VoiceChannel
from discord.backoff import ExponentialBackoff

from .ffi import VoiceConnector, VoiceConnection
from . import ffi

log = logging.getLogger(__name__)


class NativeVoiceClient(VoiceProtocol):
    """Represent a Discord voice connection

    You do not create these , you typically get them from
    e.g. :meth:`connect`

    Warnings
    ---------
    Due to datagram transmission and reception, the `opus`
    library must be installed on your system.

    Also, you need to add the location of the ffmpeg binary
    to the executable path because `ffmpeg` is used for the
    audio playback process.

    Parameters
    ------------
    client: :class:`~discord.Client`
        The client (or its subclasses) that started the connection request.
    channel: :class:`~discord.abc.Connectable`
        The voice channel that is being connected to.

    Examples
    ---------

        ::

            @commands.command()
            async def join(self, ctx: commands.Context):

                channel: discord.VoiceChannel = ctx.author.voice.channel
                if ctx.voice_client is not None:
                    return await ctx.voice_client.move_to(channel)

                await channel.connect(cls=NativeVoiceClient)

    """

    def __init__(self, client: Client, channel: VoiceChannel) -> None:
        super().__init__(client, channel)
        self._connector = VoiceConnector()
        self._connector.user_id = str(client.user.id)
        self._connection: Optional[VoiceConnection] = None
        self._guild = channel.guild
        self._attempts = 0
        self._runner: Optional[asyncio.Task] = None
        self._voice_state_received = asyncio.Event()
        self._voice_server_received = asyncio.Event()

    async def on_voice_state_update(self, data: dict) -> None:
        session_id = data['session_id']
        log.info('Voice Session ID: %s', session_id)
        self._connector.session_id = session_id
        if self._connection is not None:
            channel_id = data['channel_id']
            if channel_id is None:
                return await self.disconnect()
            else:
                self.channel = self._guild.get_channel(int(channel_id))
        else:
            self._voice_state_received.set()

    async def on_voice_server_update(self, data: dict) -> None:
        if self._voice_server_received.is_set():
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
        self._connector.update_connection_config(token, server_id, endpoint)
        self._voice_server_received.set()

    async def connect(self, *, reconnect: bool, timeout: float) -> None:
        log.info('Connecting to voice channel')
        self._voice_server_received.clear()
        self._voice_state_received.clear()
        futures = [
            self._voice_server_received.wait(),
            self._voice_state_received.wait()
        ]
        await self.voice_connect()

        try:
            await discord.utils.sane_wait_for(futures, timeout=timeout)
        except asyncio.TimeoutError:
            await self.disconnect(force=True)
            raise
        self._voice_server_received.clear()
        self._voice_state_received.clear()
        loop = asyncio.get_running_loop()
        self._connection = await self._connector.connect(loop)
        if self._runner is not None:
            self._runner.cancel()

        self._runner = loop.create_task(self.reconnect_handler(reconnect, timeout))

    async def disconnect(self, *, force: bool = False) -> None:
        try:
            if self._connection is not None:
                self._connection.disconnect()
                self._connection = None
            await self.voice_disconnect()
        finally:
            self.cleanup()

    async def move_to(self, channel: discord.abc.Connectable):
        await self.channel.guild.change_voice_state(channel=channel)

    async def voice_connect(self):
        self._attempts += 1
        await self._guild.change_voice_state(channel=self.channel)

    async def voice_disconnect(self):
        log.info('The voice handshake is being terminated for Channel ID %s (Guild ID %s)', self.channel.id, self._guild.id)
        await self._guild.change_voice_state(channel=None)

    def play(self, input: str, *, after: Callable[[Exception], None] = lambda x: None) -> None:
        """Plays **Local** audiofile

        The finalizer, ``after`` is called after the source has been exhausted
        or an error occurred.

        Parameters
        -----------
        input: `str`
            The audio source path.
        after: Callable[[Exception], None]
            The finalizer that is called after the stream is exhausted.
            This function must have a single parameter, ``error``, that
            denotes an optional exception that was raised during playing.


        """
        if self._connection:
            self._connection.play(input, after)
    
    def stop(self):
        """Stops playing audio."""
        if self._connection:
            self._connection.stop()

    def is_playing(self) -> bool:
        """Indicates if we're currently playing audio."""
        if self._connection:
            return self._connection.is_playing()
        return False

    def is_recording(self) -> bool:
        """Indicates if we're currently recording voice."""
        if self._connection:
            return self._connection.is_recording()
        return False

    def record(self, after: Callable[[Exception], None]) -> None:
        """Record discord voice stream
        
        The finalizer, ``after`` is called after the record stopped
        or an error occurred.

        Parameters
        -----------
        after: Callable[[:class:`Exception`], Any]
            The finalizer that is called after voice record is stopped.
            This function must have a single parameter, ``error``, that
            denotes an optional exception that was raised during recording.

        """
        if self._connection:
            return self._connection.record(after)

    async def stop_record(self, *, loop: Optional[asyncio.AbstractEventLoop] = None) -> Optional[bytes]:
        """|coro|
        
        Stop recording.

        From the time `record` is called to the time this function is called,
        audio data in PCM format is stored in the audio buffer in memory.

        It is recommended to call this function around 30 seconds after 
        the start of `record` due to the limitation of voice data 
        transmission capacity.

        Otherwise, the memory may be exhausted or the data may not be 
        sent correctly due to over capacity.

        Parameters
        -----------
        loop: :class:`asyncio.AbstractEventLoop`
            The event loop that the voice client is running on.

        Returns
        --------
        PCM audio buffer: Optional[bytes]

        Examples
        ---------

            ::

                @commands.command()
                async def rec(self, ctx: commands.Context):
                    ctx.voice_client.record(lambda e: print(f"Exception: {e}"))
                    
                    await ctx.send(f'Start Recording')

                    await asyncio.sleep(30)

                    await ctx.invoke(self.bot.get_command('stop'))

                @commands.command()
                async def stop(self, ctx: commands.Context):
                    await ctx.send(f'Stop Recording')

                    wav_bytes = await ctx.voice_client.stop_record()

                    wav_file = discord.File(io.BytesIO(wav_bytes), filename="Recorded.wav")

                    if wav_file:
                        await ctx.send(file=wav_file)  
            
        """
        if self._connection:
            if loop is None:
                loop = asyncio.get_event_loop()
            return await self._connection.stop_record(loop)
        return None

    def get_state(self) -> Dict:
        if self._connection:
            return self._connection.get_state()
        return {}

    async def reconnect_handler(self, reconnect, timeout):
        backoff = ExponentialBackoff()
        loop = asyncio.get_running_loop()

        while True:
            try:
                await self._connection.run(loop)
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

    @property
    def session_id(self) -> str:
        return self._connector.session_id

    @property
    def server_id(self) -> str:
        return self._connector.server_id

    @property
    def endpoint(self) -> str:
        return self._connector.endpoint

    @property
    def latency(self) -> float:
        """:class:`float`: Latency between a HEARTBEAT and a HEARTBEAT_ACK in seconds.
        This could be referred to as the Discord Voice WebSocket latency and is
        an analogue of user's voice latencies as seen in the Discord client.
        """
        return self._connection.latency if self._connection else float('inf')

    @property
    def average_latency(self) -> float:
        """:class:`float`: Average of most recent 20 HEARTBEAT latencies in seconds.
        """
        return self._connection.average_latency if self._connection else float('inf')
