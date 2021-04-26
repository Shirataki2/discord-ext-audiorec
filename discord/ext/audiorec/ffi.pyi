import asyncio
from typing import Callable, Dict

class MissingFieldError(Exception):
    pass

class InternalError(Exception):
    pass

class InternalIOError(Exception):
    pass

class TlsError(Exception):
    pass

class GatewayError(Exception):
    pass

class TryReconnect(Exception):
    pass

class VoiceConnection:
    async def run(self, loop_: asyncio.AbstractEventLoop) -> None: ...

    def disconnect(self) -> None: ...

    def stop(self) -> None: ...

    def pause(self) -> None: ...

    def resume(self) -> None: ...

    def is_playing(self) -> bool: ...
    
    def is_recording(self) -> bool: ...

    def send_playing(self) -> None: ...

    def play(self, input: str, after: Callable[[Exception], None]) -> None: ...

    def record(self, after: Callable[[Exception], None]) -> None: ...

    async def stop_record(self, loop_: asyncio.AbstractEventLoop) -> bytes: ...

    def get_state(self) -> Dict: ...

    @property
    def latency(self) -> float: ...

    @property
    def average_latency(self) -> float: ...

class VoiceConnector:
    session_id: str
    user_id: str

    @property
    def server_id(self) -> str: ...

    @property
    def endpoint(self) -> str: ...

    def __init__(self) -> None: ...

    def update_connection_config(
        self,
        token: str,
        server_id: str,
        endpoint: str,
    ) -> None:
        ...

    async def connect(self, loop_: asyncio.AbstractEventLoop) -> VoiceConnection: ...

    async def disconnect(self) -> None: ...