"""
Meshag - Distributed Frame-Based Pipeline for Voice AI

A Python library for building distributed voice AI pipelines with support for
custom processors, WebSocket communication, and priority-based frame processing.
"""

from .meshag import (
    PyTextFrame as TextFrame,
    PyAudioFrame as AudioFrame,
    PyPipeline as Pipeline,
    PyRunner as Runner,
)

__all__ = [
    "TextFrame",
    "AudioFrame",
    "Pipeline",
    "Runner",
]

__version__ = "0.1.0"
