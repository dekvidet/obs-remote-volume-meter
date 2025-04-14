import { useState, useEffect } from 'react'
import './App.css'
import VuMeter from './components/VuMeter./VuMeter'

function App() {
  const [volume, setVolume] = useState(0)
  useEffect(() => {
    const interval = setInterval(() => {
      setVolume(Math.floor(Math.random() * 101))
    }, 200)
    return () => clearInterval(interval)
  }, [])


  return (
    <>
      <div>
        <VuMeter volume={volume}/>
      </div>
    </>
  )
}

export default App
